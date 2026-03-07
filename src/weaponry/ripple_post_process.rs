//! Post-process ripple displacement effect.
//!
//! Each camera with [`RipplePostProcess`] gets a fullscreen distortion pass after tonemapping.
//! Ripple entries live in a shared GPU storage buffer (unbounded count), while each camera
//! keeps a slim uniform with its projection parameters and the current ripple count.

use bevy::{
    core_pipeline::{
        core_2d::graph::{Core2d, Node2d},
        FullscreenShader,
    },
    ecs::query::QueryItem,
    render::{
        extract_component::{
            ComponentUniforms, DynamicUniformIndex, ExtractComponent, ExtractComponentPlugin,
            UniformComponentPlugin,
        },
        extract_resource::{ExtractResource, ExtractResourcePlugin},
        render_graph::{
            NodeRunError, RenderGraphContext, RenderGraphExt, RenderLabel, ViewNode, ViewNodeRunner,
        },
        render_resource::{
            binding_types::{sampler, storage_buffer_read_only, texture_2d, uniform_buffer},
            *,
        },
        renderer::{RenderContext, RenderDevice, RenderQueue},
        view::ViewTarget,
        Render, RenderApp, RenderSystems, RenderStartup,
    },
};
use super::ripple::Ripple;
use crate::prelude::*;

pub struct RipplePostProcessPlugin;
impl Plugin for RipplePostProcessPlugin {
    fn build(&self, app: &mut App) {
        app
            .add_plugins((
                ExtractComponentPlugin::<RipplePostProcess>::default(),
                UniformComponentPlugin::<RipplePostProcess>::default(),
                ExtractResourcePlugin::<RippleEntries>::default(),
            ))
            .init_resource::<RippleEntries>()
            .add_observer(RipplePostProcess::on_add_post_process_camera)
            .add_systems(Update, RipplePostProcess::update)
            ;

        let Some(render_app) = app.get_sub_app_mut(RenderApp) else { return; };
        render_app
            .init_resource::<GpuRippleStorage>()
            .add_systems(RenderStartup, init_ripple_pipeline)
            .add_systems(Render, GpuRippleStorage::prepare.in_set(RenderSystems::PrepareResources))
            .add_render_graph_node::<ViewNodeRunner<RipplePostProcessNode>>(
                Core2d,
                RipplePostProcessLabel,
            )
            .add_render_graph_edges(
                Core2d,
                (
                    Node2d::Tonemapping,
                    RipplePostProcessLabel,
                    Node2d::EndMainPassPostProcessing,
                ),
            );
    }
}

// ── GPU data ──────────────────────────────────────────────────────────────────────────

/// 16-byte per-ripple entry uploaded to the shared storage buffer.
#[derive(Clone, Copy, ShaderType, Default)]
struct GpuRippleEntry {
    center: Vec2,
    normalized_radius: f32,
    max_radius: f32,
}

/// Wrapper for the storage buffer. The `#[shader(size(runtime))]` attribute tells encase
/// that `entries` is a runtime-sized array — the GPU buffer grows to fit however many
/// ripples are active this frame.
#[derive(ShaderType, Default)]
struct GpuRippleBuffer {
    #[shader(size(runtime))]
    entries: Vec<GpuRippleEntry>,
}

/// Main-world resource holding this frame's ripple entries.
/// `ExtractResourcePlugin` clones this into the render world each frame (the trait
/// receives `&Source`, so the clone at the extraction boundary is unavoidable).
#[derive(Resource, Default, Clone)]
struct RippleEntries(Vec<GpuRippleEntry>);
impl ExtractResource for RippleEntries {
    type Source = RippleEntries;
    fn extract_resource(source: &Self::Source) -> Self { source.clone() }
}

/// GPU-side storage buffer holding all active ripple entries (unbounded).
#[derive(Resource, Default)]
struct GpuRippleStorage {
    buffer: StorageBuffer<GpuRippleBuffer>,
}
impl GpuRippleStorage {
    /// Takes the extracted entries and writes them to the GPU storage buffer.
    fn prepare(
        mut extracted: ResMut<RippleEntries>,
        render_device: Res<RenderDevice>,
        render_queue: Res<RenderQueue>,
        mut storage: ResMut<GpuRippleStorage>,
    ) {
        // Take instead of clone — the extracted resource is recreated each frame anyway.
        let mut entries = std::mem::take(&mut extracted.0);
        // Some GPU backends reject 0-byte storage buffers, so always keep at least one
        // (dummy) entry. The shader early-returns when ripple_count == 0.
        if entries.is_empty() {
            entries.push(GpuRippleEntry::default());
        }
        storage.buffer.set(GpuRippleBuffer { entries });
        storage.buffer.write_buffer(&render_device, &render_queue);
    }
}

/// Per-camera uniform. Only carries projection parameters and the shared ripple count.
#[derive(Component, ExtractComponent, Clone, Copy, ShaderType, Default)]
pub struct RipplePostProcess {
    camera_world_pos: Vec2,
    viewport_world_size: Vec2,
    ripple_count: u32,
}
impl RipplePostProcess {
    fn on_add_post_process_camera(
        trigger: On<Add, lib_core::camera::PostProcessCamera>,
        mut commands: Commands,
    ) {
        commands.entity(trigger.entity).insert(RipplePostProcess::default());
    }

    fn update(
        mut entries: ResMut<RippleEntries>,
        ripples: Query<(&Ripple, &Transform)>,
        mut cameras: Query<(&mut RipplePostProcess, &Transform, &Projection)>,
    ) {
        entries.0.clear();
        for (ripple, transform) in ripples.iter() {
            entries.0.push(GpuRippleEntry {
                center: transform.translation.xy(),
                normalized_radius: ripple.normalized_radius(),
                max_radius: ripple.max_radius(),
            });
        }

        let count = entries.0.len() as u32;
        for (mut post_process, transform, projection) in cameras.iter_mut() {
            let Projection::Orthographic(ortho) = &*projection else { continue; };
            post_process.camera_world_pos = transform.translation.xy();
            post_process.viewport_world_size = Vec2::new(
                ortho.area.width(),
                ortho.area.height(),
            );
            post_process.ripple_count = count;
        }
    }
}

// ── Render graph ──────────────────────────────────────────────────────────────────────

#[derive(Debug, Hash, PartialEq, Eq, Clone, RenderLabel)]
struct RipplePostProcessLabel;

#[derive(Default)]
struct RipplePostProcessNode;
impl ViewNode for RipplePostProcessNode {
    type ViewQuery = (
        &'static ViewTarget,
        &'static DynamicUniformIndex<RipplePostProcess>,
        &'static RipplePostProcess,
    );

    fn run<'w>(
        &self,
        _graph: &mut RenderGraphContext,
        render_context: &mut RenderContext<'w>,
        (view_target, settings_index, settings): QueryItem<'w, 'w, Self::ViewQuery>,
        world: &'w World,
    ) -> Result<(), NodeRunError> {
        // Skip the entire GPU dispatch when no ripples are active.
        if settings.ripple_count == 0 {
            return Ok(());
        }
        let pipeline_res = world.resource::<RipplePostProcessPipeline>();
        let pipeline_cache = world.resource::<PipelineCache>();

        // Pick the pipeline variant matching the camera's texture format.
        let pipeline_id = if view_target.main_texture_format() == ViewTarget::TEXTURE_FORMAT_HDR {
            pipeline_res.pipeline_id_hdr
        } else {
            pipeline_res.pipeline_id_ldr
        };
        // Shaders compile asynchronously; skip gracefully while still compiling.
        let Some(pipeline) = pipeline_cache.get_render_pipeline(pipeline_id) else {
            return Ok(());
        };

        let settings_uniforms = world.resource::<ComponentUniforms<RipplePostProcess>>();
        let Some(settings_binding) = settings_uniforms.uniforms().binding() else {
            return Ok(());
        };

        let ripple_storage = world.resource::<GpuRippleStorage>();
        let Some(storage_binding) = ripple_storage.buffer.binding() else {
            return Ok(());
        };

        // Ping-pong: reads from `source`, writes to `destination`, then swaps for the
        // next post-process pass in the chain.
        let post_process = view_target.post_process_write();

        let bind_group = render_context.render_device().create_bind_group(
            "ripple_post_process_bind_group",
            &pipeline_cache.get_bind_group_layout(&pipeline_res.layout),
            &BindGroupEntries::sequential((
                post_process.source,
                &pipeline_res.sampler,
                settings_binding.clone(),
                storage_binding.clone(),
            )),
        );

        let mut render_pass = render_context.begin_tracked_render_pass(RenderPassDescriptor {
            label: Some("ripple_post_process_pass"),
            color_attachments: &[Some(RenderPassColorAttachment {
                view: post_process.destination,
                depth_slice: None,
                resolve_target: None,
                ops: Operations::default(),
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
        });

        render_pass.set_render_pipeline(pipeline);
        // Dynamic offset selects this camera's uniform slice from the shared buffer.
        render_pass.set_bind_group(0, &bind_group, &[settings_index.index()]);
        // 3 vertices = Bevy's built-in fullscreen triangle (no vertex buffer needed).
        render_pass.draw(0..3, 0..1);

        Ok(())
    }
}

// ── Pipeline ──────────────────────────────────────────────────────────────────────────

#[derive(Resource)]
struct RipplePostProcessPipeline {
    layout: BindGroupLayoutDescriptor,
    sampler: Sampler,
    pipeline_id_ldr: CachedRenderPipelineId,
    pipeline_id_hdr: CachedRenderPipelineId,
}

fn init_ripple_pipeline(
    mut commands: Commands,
    render_device: Res<RenderDevice>,
    asset_server: Res<AssetServer>,
    fullscreen_shader: Res<FullscreenShader>,
    pipeline_cache: Res<PipelineCache>,
) {
    let layout = BindGroupLayoutDescriptor::new(
        "ripple_post_process_bind_group_layout",
        &BindGroupLayoutEntries::sequential(
            ShaderStages::FRAGMENT,
            (
                texture_2d(TextureSampleType::Float { filterable: true }),
                sampler(SamplerBindingType::Filtering),
                // `true` = dynamic offset (per-camera slice in the shared uniform buffer).
                uniform_buffer::<RipplePostProcess>(true),
                // `false` = no dynamic offset (single shared buffer, all cameras read the same data).
                storage_buffer_read_only::<GpuRippleEntry>(false),
            ),
        ),
    );

    let sampler = render_device.create_sampler(&SamplerDescriptor::default());
    let shader = asset_server.load("shaders/ripple_post_process.wgsl");

    let make_pipeline = |format| RenderPipelineDescriptor {
        label: Some("ripple_post_process_pipeline".into()),
        layout: vec![layout.clone()],
        vertex: fullscreen_shader.to_vertex_state(),
        fragment: Some(FragmentState {
            shader: shader.clone(),
            targets: vec![Some(ColorTargetState {
                format,
                blend: None,
                write_mask: ColorWrites::ALL,
            })],
            ..default()
        }),
        ..default()
    };

    let pipeline_id_ldr = pipeline_cache.queue_render_pipeline(make_pipeline(TextureFormat::bevy_default()));
    let pipeline_id_hdr = pipeline_cache.queue_render_pipeline(make_pipeline(ViewTarget::TEXTURE_FORMAT_HDR));

    commands.insert_resource(RipplePostProcessPipeline {
        layout,
        sampler,
        pipeline_id_ldr,
        pipeline_id_hdr,
    });
}
