//! # Inferno Haze
//!
//! Screen-space heat-haze distortion around charging `TowerInferno` entities, applied after tonemapping.
//!
//! Shimmer rings converge inward from one tile outside the tower footprint toward the center,
//! reaching the core at full charge.
//!
//! Active entries are uploaded to a shared GPU storage buffer; each `InfernoHazePostProcess` camera
//! stores projection parameters, time, and entry count.

use bevy::{
    core_pipeline::{FullscreenShader, schedule::Core2d},
    prelude::*,
    render::{
        Render, RenderApp, RenderStartup, RenderSystems,
        camera::ExtractedCamera,
        extract_component::{
            ComponentUniforms, DynamicUniformIndex, ExtractComponent, ExtractComponentPlugin,
            UniformComponentPlugin,
        },
        extract_resource::{ExtractResource, ExtractResourcePlugin},
        render_resource::{
            *,
            binding_types::{sampler, storage_buffer_read_only, texture_2d, uniform_buffer},
        },
        renderer::{RenderContext, RenderDevice, RenderQueue, ViewQuery},
        view::ViewTarget,
    },
};

use buildings::prelude::*;
use game_core::prelude::*;
use viewport::PostProcessCamera;
use visuals::post_process::InfernoHazePostProcessSet;

pub(crate) struct TowerInfernoHazePlugin;
impl Plugin for TowerInfernoHazePlugin {
    fn build(&self, app: &mut App) {
        app
            .add_plugins((
                ExtractComponentPlugin::<InfernoHazePostProcess>::default(),
                UniformComponentPlugin::<InfernoHazePostProcess>::default(),
                ExtractResourcePlugin::<InfernoHazeEntries>::default(),
            ))
            .init_resource::<InfernoHazeEntries>()
            .add_observer(InfernoHazePostProcess::on_add_camera_attach_post_process)
            .add_systems(Update, InfernoHazePostProcess::update);

        let Some(render_app) = app.get_sub_app_mut(RenderApp) else { return; };
        render_app
            .init_resource::<GpuInfernoHazeStorage>()
            .add_systems(RenderStartup, init_inferno_haze_pipeline)
            .add_systems(Render, GpuInfernoHazeStorage::prepare.in_set(RenderSystems::PrepareResources))
            // Ordering against the other post-process passes lives in the
            // visuals_internal's PostProcessOrderingPlugin (added after all effect plugins).
            .add_systems(Core2d, inferno_haze_post_process_pass.in_set(InfernoHazePostProcessSet));
    }
}

/// Haze extent beyond the tower footprint (1 tile).
const HAZE_MARGIN: f32 = CELL_SIZE;

// ── GPU data ──────────────────────────────────────────────────────────────────────────

/// Per-tower storage buffer entry. Field order and types mirror `InfernoHazeEntry` in
/// `assets/shaders/towers/inferno/haze_post_process.wesl`.
#[derive(Clone, Copy, ShaderType, Default)]
struct GpuInfernoHazeEntry {
    center: Vec2,
    /// Outer radius of the haze in world units.
    reach: f32,
    charge: f32,
}

#[derive(ShaderType, Default)]
struct GpuInfernoHazeBuffer {
    #[shader(size(runtime))]
    entries: Vec<GpuInfernoHazeEntry>,
}

/// Main-world charging Inferno entries, cloned into the render world each frame.
#[derive(Resource, Default, Clone)]
struct InfernoHazeEntries(Vec<GpuInfernoHazeEntry>);
impl ExtractResource<RenderApp> for InfernoHazeEntries {
    type Source = InfernoHazeEntries;
    fn extract_resource(source: &Self::Source) -> Self { source.clone() }
}

#[derive(Resource, Default)]
struct GpuInfernoHazeStorage {
    buffer: StorageBuffer<GpuInfernoHazeBuffer>,
}
impl GpuInfernoHazeStorage {
    fn prepare(
        mut storage: ResMut<GpuInfernoHazeStorage>,
        render_device: Res<RenderDevice>,
        render_queue: Res<RenderQueue>,
        mut extracted: ResMut<InfernoHazeEntries>,
    ) {
        let mut entries = std::mem::take(&mut extracted.0);
        // Some GPU backends reject 0-byte storage buffers; keep a dummy entry when empty.
        // The render pass is skipped when `inferno_count == 0`.
        if entries.is_empty() {
            entries.push(GpuInfernoHazeEntry::default());
        }
        storage.buffer.set(GpuInfernoHazeBuffer { entries });
        storage.buffer.write_buffer(&render_device, &render_queue);
    }
}

/// Per-camera uniform. Field order and types mirror `CameraData` in
/// `assets/shaders/towers/inferno/haze_post_process.wesl`.
#[derive(Component, ExtractComponent, Clone, Copy, ShaderType, Default)]
#[extract_app(RenderApp)]
pub(crate) struct InfernoHazePostProcess {
    camera_world_pos: Vec2,
    viewport_world_size: Vec2,
    time: f32,
    inferno_count: u32,
}
impl InfernoHazePostProcess {
    fn on_add_camera_attach_post_process(
        trigger: On<Add<PostProcessCamera>>,
        mut commands: Commands,
    ) {
        commands.entity(trigger.entity).insert(InfernoHazePostProcess::default());
    }

    fn update(
        time: Res<Time>,
        mut entries: ResMut<InfernoHazeEntries>,
        towers: Query<(&Transform, &GridImprint, &TowerShootingTimer), With<TowerInferno>>,
        mut cameras: Query<(&mut InfernoHazePostProcess, &Transform, &Projection)>,
    ) {
        entries.0.clear();
        for (transform, grid_imprint, timer) in towers.iter() {
            let charge = timer.0.fraction();
            if charge <= 0. { continue; }
            entries.0.push(GpuInfernoHazeEntry {
                center: transform.translation.xy(),
                reach: grid_imprint.world_size().max_element() * 0.5 + HAZE_MARGIN,
                charge,
            });
        }

        let inferno_count = entries.0.len() as u32;
        for (mut post_process, transform, projection) in cameras.iter_mut() {
            let Projection::Orthographic(orthographic) = projection else { continue; };
            post_process.camera_world_pos = transform.translation.xy();
            post_process.viewport_world_size = Vec2::new(orthographic.area.width(), orthographic.area.height());
            post_process.time = time.elapsed_secs_wrapped();
            post_process.inferno_count = inferno_count;
        }
    }
}

// ── Render pass system ───────────────────────────────────────────────────────────────

fn inferno_haze_post_process_pass(
    view: ViewQuery<(
        &ViewTarget,
        &DynamicUniformIndex<InfernoHazePostProcess>,
        &InfernoHazePostProcess,
        &ExtractedCamera,
    )>,
    post_process_pipeline: Res<InfernoHazePostProcessPipeline>,
    pipeline_cache: Res<PipelineCache>,
    settings_uniforms: Res<ComponentUniforms<InfernoHazePostProcess>>,
    haze_storage: Res<GpuInfernoHazeStorage>,
    mut render_context: RenderContext,
) {
    let (view_target, settings_index, settings, camera) = view.into_inner();
    if settings.inferno_count == 0 { return; }

    // The post-process passes only run on HDR cameras (Rgba16Float framebuffer).
    assert!(
        camera.hdr,
        "inferno haze post-process requires an HDR camera; this PostProcessCamera lacks `Hdr`. \
         Add the `Hdr` component, or add a pipeline variant for its target format."
    );
    // Shaders compile asynchronously; skip while still compiling.
    let Some(pipeline) = pipeline_cache.get_render_pipeline(post_process_pipeline.pipeline_id) else { return; };
    let Some(settings_binding) = settings_uniforms.uniforms().binding() else { return; };
    let Some(storage_binding) = haze_storage.buffer.binding() else { return; };

    // Ping-pong: reads from `source`, writes to `destination`, then swaps for the next pass.
    let post_process = view_target.post_process_write();

    let bind_group = render_context.render_device().create_bind_group(
        "inferno_haze_post_process_bind_group",
        &pipeline_cache.get_bind_group_layout(&post_process_pipeline.layout),
        &BindGroupEntries::sequential((
            post_process.source,
            &post_process_pipeline.sampler,
            settings_binding.clone(),
            storage_binding.clone(),
        )),
    );

    let mut render_pass = render_context
        .command_encoder()
        .begin_render_pass(&RenderPassDescriptor {
            label: Some("inferno_haze_post_process_pass"),
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
    // 3 vertices = Bevy's built-in fullscreen triangle.
    render_pass.draw(0..3, 0..1);
}

// ── Pipeline ──────────────────────────────────────────────────────────────────────────

#[derive(Resource)]
struct InfernoHazePostProcessPipeline {
    layout: BindGroupLayoutDescriptor,
    sampler: Sampler,
    pipeline_id: CachedRenderPipelineId,
}

fn init_inferno_haze_pipeline(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    fullscreen_shader: Res<FullscreenShader>,
    pipeline_cache: Res<PipelineCache>,
    render_device: Res<RenderDevice>,
) {
    let layout = BindGroupLayoutDescriptor::new(
        "inferno_haze_post_process_bind_group_layout",
        &BindGroupLayoutEntries::sequential(
            ShaderStages::FRAGMENT,
            (
                texture_2d(TextureSampleType::Float { filterable: true }),
                sampler(SamplerBindingType::Filtering),
                // `true` = dynamic offset (per-camera slice in the shared uniform buffer).
                uniform_buffer::<InfernoHazePostProcess>(true),
                storage_buffer_read_only::<GpuInfernoHazeEntry>(false),
            ),
        ),
    );

    // Sub-pixel UV offsets require linear filtering.
    let sampler = render_device.create_sampler(&SamplerDescriptor {
        mag_filter: FilterMode::Linear,
        min_filter: FilterMode::Linear,
        ..default()
    });
    let shader = asset_server.load("shaders/towers/inferno/haze_post_process.wesl");

    let pipeline_id = pipeline_cache.queue_render_pipeline(RenderPipelineDescriptor {
        label: Some("inferno_haze_post_process_pipeline".into()),
        layout: vec![layout.clone()],
        vertex: fullscreen_shader.to_vertex_state(),
        fragment: Some(FragmentState {
            shader,
            targets: vec![Some(ColorTargetState {
                format: TextureFormat::Rgba16Float,
                blend: None,
                write_mask: ColorWrites::ALL,
            })],
            ..default()
        }),
        ..default()
    });

    commands.insert_resource(InfernoHazePostProcessPipeline {
        layout,
        sampler,
        pipeline_id,
    });
}
