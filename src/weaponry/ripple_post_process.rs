//! Post-process ripple displacement effect.
//!
//! Each camera with [`RipplePostProcess`] gets a fullscreen distortion pass after tonemapping.
//! Ripple entries live in a shared GPU storage buffer (unbounded count), while each camera
//! keeps a slim uniform with its projection parameters and the current ripple count.

use super::ripple::Ripple;
use crate::prelude::*;
use bevy::{
    core_pipeline::{FullscreenShader, schedule::Core2d},
    render::{
        Render, RenderApp, RenderStartup, RenderSystems,
        camera::ExtractedCamera,
        extract_component::{
            ComponentUniforms, DynamicUniformIndex, ExtractComponent, ExtractComponentPlugin,
            UniformComponentPlugin,
        },
        extract_resource::{ExtractResource, ExtractResourcePlugin},
        render_resource::{
            binding_types::{sampler, storage_buffer_read_only, texture_2d, uniform_buffer},
            *,
        },
        renderer::{RenderContext, RenderDevice, RenderQueue, ViewQuery},
        view::ViewTarget,
    },
};
use lib_core::post_processing::RipplePostProcessSet;

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
            // Ordering against the other post-process passes lives in lib-core's
            // PostProcessOrderingPlugin (added after all effect plugins).
            .add_systems(Core2d, ripple_post_process_pass.in_set(RipplePostProcessSet));
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

// ── Render pass system ───────────────────────────────────────────────────────────────

fn ripple_post_process_pass(
    view: ViewQuery<(
        &ViewTarget,
        &DynamicUniformIndex<RipplePostProcess>,
        &RipplePostProcess,
        &ExtractedCamera,
    )>,
    pipeline_res: Res<RipplePostProcessPipeline>,
    pipeline_cache: Res<PipelineCache>,
    settings_uniforms: Res<ComponentUniforms<RipplePostProcess>>,
    ripple_storage: Res<GpuRippleStorage>,
    mut ctx: RenderContext,
) {
    let (view_target, settings_index, settings, camera) = view.into_inner();

    // Skip the entire GPU dispatch when no ripples are active.
    if settings.ripple_count == 0 {
        return;
    }

    // The post-process passes only run on HDR cameras (Rgba16Float framebuffer).
    assert!(
        camera.hdr,
        "ripple post-process requires an HDR camera; this PostProcessCamera lacks `Hdr`. \
         Add the `Hdr` component, or add a pipeline variant for its target format."
    );
    let pipeline_id = pipeline_res.pipeline_id;
    // Shaders compile asynchronously; skip gracefully while still compiling.
    let Some(pipeline) = pipeline_cache.get_render_pipeline(pipeline_id) else { return; };
    let Some(settings_binding) = settings_uniforms.uniforms().binding() else { return; };
    let Some(storage_binding) = ripple_storage.buffer.binding() else { return; };

    // Ping-pong: reads from `source`, writes to `destination`, then swaps for the
    // next post-process pass in the chain.
    let post_process = view_target.post_process_write();

    let bind_group = ctx.render_device().create_bind_group(
        "ripple_post_process_bind_group",
        &pipeline_cache.get_bind_group_layout(&pipeline_res.layout),
        &BindGroupEntries::sequential((
            post_process.source,
            &pipeline_res.sampler,
            settings_binding.clone(),
            storage_binding.clone(),
        )),
    );

    let mut render_pass = ctx
        .command_encoder()
        .begin_render_pass(&RenderPassDescriptor {
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
            multiview_mask: None,
        });

    render_pass.set_pipeline(pipeline);
    // Dynamic offset selects this camera's uniform slice from the shared buffer.
    render_pass.set_bind_group(0, &bind_group, &[settings_index.index()]);
    // 3 vertices = Bevy's built-in fullscreen triangle (no vertex buffer needed).
    render_pass.draw(0..3, 0..1);
}

// ── Pipeline ──────────────────────────────────────────────────────────────────────────

#[derive(Resource)]
struct RipplePostProcessPipeline {
    layout: BindGroupLayoutDescriptor,
    sampler: Sampler,
    pipeline_id: CachedRenderPipelineId,
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

    let pipeline_id =
        pipeline_cache.queue_render_pipeline(make_pipeline(TextureFormat::Rgba16Float));

    commands.insert_resource(RipplePostProcessPipeline {
        layout,
        sampler,
        pipeline_id,
    });
}
