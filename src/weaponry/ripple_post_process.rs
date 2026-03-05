//! Post-process ripple displacement effect.
//!
//! Each camera with [`RipplePostProcess`] gets a fullscreen distortion pass after tonemapping.
//! All active ripples are accumulated in a single shader invocation — no flickering between them.
//! Works on all cameras (main, preview, drone) without paired capture cameras.

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
        render_graph::{
            NodeRunError, RenderGraphContext, RenderGraphExt, RenderLabel, ViewNode, ViewNodeRunner,
        },
        render_resource::{
            binding_types::{sampler, texture_2d, uniform_buffer},
            *,
        },
        renderer::{RenderContext, RenderDevice},
        view::ViewTarget,
        RenderApp, RenderStartup,
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
            ))
            .add_observer(RipplePostProcess::on_add_main_camera)
            .add_observer(RipplePostProcess::on_add_builder_preview_camera)
            .add_systems(Update, RipplePostProcess::update)
            ;

        let Some(render_app) = app.get_sub_app_mut(RenderApp) else { return; };
        render_app
            .add_systems(RenderStartup, init_ripple_pipeline)
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

// ── GPU data structs ──────────────────────────────────────────────────────────────────

const MAX_RIPPLES: usize = 32;

/// Per-ripple GPU entry. Two Vec4 = 32 bytes with guaranteed 16-byte alignment,
/// satisfying WGSL uniform array stride requirements.
///
/// `data0`: xy = world-space center, z = normalized_radius (0..0.5), w = mesh_half_size
/// `data1`: x = wave_width (0.15), yzw = padding
#[derive(Clone, Copy, ShaderType, Default)]
pub struct GpuRippleEntry {
    pub data0: Vec4,
    pub data1: Vec4,
}

/// Post-process settings component. Added to cameras that should render ripple distortion.
/// Extracted and uploaded to a dynamic uniform buffer each frame (one entry per camera),
/// which is why each camera can have independent `camera_world_pos` / `viewport_world_size`.
///
/// Memory layout (1056 bytes):
/// - `camera_world_pos` + `viewport_world_size` → 16 bytes
/// - `count_and_pad` (UVec4, x = count) → 16 bytes
/// - `entries` (32 × 32-byte GpuRippleEntry) → 1024 bytes
#[derive(Component, ExtractComponent, Clone, Copy, ShaderType, Default)]
pub struct RipplePostProcess {
    pub camera_world_pos: Vec2,
    pub viewport_world_size: Vec2,
    pub count_and_pad: UVec4,
    pub entries: [GpuRippleEntry; MAX_RIPPLES],
}
impl RipplePostProcess {
    fn on_add_main_camera(
        trigger: On<Add, lib_core::camera::MainCamera>,
        mut commands: Commands,
    ) {
        commands.entity(trigger.entity).insert(RipplePostProcess::default());
    }

    fn on_add_builder_preview_camera(
        trigger: On<Add, lib_core::camera::BuilderPreviewCamera>,
        mut commands: Commands,
    ) {
        commands.entity(trigger.entity).insert(RipplePostProcess::default());
    }

    fn update(
        ripples: Query<(&Ripple, &Transform)>,
        mut cameras: Query<(&mut RipplePostProcess, &Transform, &Projection)>,
    ) {
        let mut entries = [GpuRippleEntry::default(); MAX_RIPPLES];
        let mut count = 0u32;

        for (ripple, transform) in ripples.iter() {
            if count as usize >= MAX_RIPPLES { break; }
            entries[count as usize] = GpuRippleEntry {
                data0: Vec4::new(
                    transform.translation.x,
                    transform.translation.y,
                    ripple.normalized_radius(),
                    ripple.max_radius(),
                ),
                data1: Vec4::new(0.15, 0.0, 0.0, 0.0),
            };
            count += 1;
        }

        for (mut post_process, transform, projection) in cameras.iter_mut() {
            let Projection::Orthographic(ortho) = &*projection else { continue; };
            post_process.camera_world_pos = transform.translation.xy();
            post_process.viewport_world_size = Vec2::new(
                ortho.area.width(),
                ortho.area.height(),
            );
            post_process.count_and_pad = UVec4::new(count, 0, 0, 0);
            post_process.entries = entries;
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
    );

    fn run<'w>(
        &self,
        _graph: &mut RenderGraphContext,
        render_context: &mut RenderContext<'w>,
        (view_target, settings_index): QueryItem<'w, 'w, Self::ViewQuery>,
        world: &'w World,
    ) -> Result<(), NodeRunError> {
        let pipeline_res = world.resource::<RipplePostProcessPipeline>();
        let pipeline_cache = world.resource::<PipelineCache>();

        let pipeline_id = if view_target.main_texture_format() == ViewTarget::TEXTURE_FORMAT_HDR {
            pipeline_res.pipeline_id_hdr
        } else {
            pipeline_res.pipeline_id_ldr
        };
        let Some(pipeline) = pipeline_cache.get_render_pipeline(pipeline_id) else {
            return Ok(());
        };

        let settings_uniforms = world.resource::<ComponentUniforms<RipplePostProcess>>();
        let Some(settings_binding) = settings_uniforms.uniforms().binding() else {
            return Ok(());
        };

        let post_process = view_target.post_process_write();

        let bind_group = render_context.render_device().create_bind_group(
            "ripple_post_process_bind_group",
            &pipeline_cache.get_bind_group_layout(&pipeline_res.layout),
            &BindGroupEntries::sequential((
                post_process.source,
                &pipeline_res.sampler,
                settings_binding.clone(),
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
        render_pass.set_bind_group(0, &bind_group, &[settings_index.index()]);
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
                uniform_buffer::<RipplePostProcess>(true),
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
