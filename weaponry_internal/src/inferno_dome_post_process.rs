//! # Inferno Dome Distortion
//!
//! Screen-space distortion pass for `InfernoDome` entities, applied after tonemapping:
//! - Radial lens bulge inside the dome during breakup (`0..LENS_DURATION`).
//! - Heat shimmer across the dome footprint (`0..SHIMMER_DURATION`).
//!
//! Active domes are uploaded to a shared GPU storage buffer; each `InfernoDomePostProcess` camera
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

use viewport::PostProcessCamera;
use visuals::post_process::InfernoDomePostProcessSet;
use weaponry::prelude::InfernoDome;

pub(crate) struct InfernoDomePostProcessPlugin;
impl Plugin for InfernoDomePostProcessPlugin {
    fn build(&self, app: &mut App) {
        app
            .add_plugins((
                ExtractComponentPlugin::<InfernoDomePostProcess>::default(),
                UniformComponentPlugin::<InfernoDomePostProcess>::default(),
                ExtractResourcePlugin::<InfernoDomeEntries>::default(),
            ))
            .init_resource::<InfernoDomeEntries>()
            .add_observer(InfernoDomePostProcess::on_add_camera_attach_post_process)
            .add_systems(Update, InfernoDomePostProcess::update);

        let Some(render_app) = app.get_sub_app_mut(RenderApp) else { return; };
        render_app
            .init_resource::<GpuInfernoDomeStorage>()
            .add_systems(RenderStartup, init_inferno_dome_pipeline)
            .add_systems(Render, GpuInfernoDomeStorage::prepare.in_set(RenderSystems::PrepareResources))
            // Ordering against the other post-process passes lives in the
            // visuals_internal's PostProcessOrderingPlugin (added after all effect plugins).
            .add_systems(Core2d, inferno_dome_post_process_pass.in_set(InfernoDomePostProcessSet));
    }
}

// ── GPU data ──────────────────────────────────────────────────────────────────────────

/// Per-dome storage buffer entry. Field order and types mirror `InfernoDomeEntry` in
/// `assets/shaders/weaponry/inferno_dome_post_process.wesl`.
#[derive(Clone, Copy, ShaderType, Default)]
struct GpuInfernoDomeEntry {
    center: Vec2,
    radius: f32,
    age: f32,
}

#[derive(ShaderType, Default)]
struct GpuInfernoDomeBuffer {
    #[shader(size(runtime))]
    entries: Vec<GpuInfernoDomeEntry>,
}

/// Main-world dome entries, cloned into the render world each frame.
#[derive(Resource, Default, Clone)]
struct InfernoDomeEntries(Vec<GpuInfernoDomeEntry>);
impl ExtractResource<RenderApp> for InfernoDomeEntries {
    type Source = InfernoDomeEntries;
    fn extract_resource(source: &Self::Source) -> Self { source.clone() }
}

#[derive(Resource, Default)]
struct GpuInfernoDomeStorage {
    buffer: StorageBuffer<GpuInfernoDomeBuffer>,
}
impl GpuInfernoDomeStorage {
    fn prepare(
        mut storage: ResMut<GpuInfernoDomeStorage>,
        render_device: Res<RenderDevice>,
        render_queue: Res<RenderQueue>,
        mut extracted: ResMut<InfernoDomeEntries>,
    ) {
        let mut entries = std::mem::take(&mut extracted.0);
        // Some GPU backends reject 0-byte storage buffers; keep a dummy entry when empty.
        // The render pass is skipped when `dome_count == 0`.
        if entries.is_empty() {
            entries.push(GpuInfernoDomeEntry::default());
        }
        storage.buffer.set(GpuInfernoDomeBuffer { entries });
        storage.buffer.write_buffer(&render_device, &render_queue);
    }
}

/// Per-camera uniform. Field order and types mirror `CameraData` in
/// `assets/shaders/weaponry/inferno_dome_post_process.wesl`.
#[derive(Component, ExtractComponent, Clone, Copy, ShaderType, Default)]
#[extract_app(RenderApp)]
pub(crate) struct InfernoDomePostProcess {
    camera_world_pos: Vec2,
    viewport_world_size: Vec2,
    time: f32,
    dome_count: u32,
}
impl InfernoDomePostProcess {
    fn on_add_camera_attach_post_process(
        trigger: On<Add<PostProcessCamera>>,
        mut commands: Commands,
    ) {
        commands.entity(trigger.entity).insert(InfernoDomePostProcess::default());
    }

    fn update(
        time: Res<Time>,
        mut entries: ResMut<InfernoDomeEntries>,
        domes: Query<(&Transform, &InfernoDome)>,
        mut cameras: Query<(&mut InfernoDomePostProcess, &Transform, &Projection)>,
    ) {
        entries.0.clear();
        for (transform, dome) in domes.iter() {
            entries.0.push(GpuInfernoDomeEntry {
                center: transform.translation.xy(),
                radius: dome.radius,
                age: dome.age,
            });
        }

        let dome_count = entries.0.len() as u32;
        for (mut post_process, transform, projection) in cameras.iter_mut() {
            let Projection::Orthographic(orthographic) = projection else { continue; };
            post_process.camera_world_pos = transform.translation.xy();
            post_process.viewport_world_size = Vec2::new(orthographic.area.width(), orthographic.area.height());
            post_process.time = time.elapsed_secs_wrapped();
            post_process.dome_count = dome_count;
        }
    }
}

// ── Render pass system ───────────────────────────────────────────────────────────────

fn inferno_dome_post_process_pass(
    view: ViewQuery<(
        &ViewTarget,
        &DynamicUniformIndex<InfernoDomePostProcess>,
        &InfernoDomePostProcess,
        &ExtractedCamera,
    )>,
    post_process_pipeline: Res<InfernoDomePostProcessPipeline>,
    pipeline_cache: Res<PipelineCache>,
    settings_uniforms: Res<ComponentUniforms<InfernoDomePostProcess>>,
    dome_storage: Res<GpuInfernoDomeStorage>,
    mut render_context: RenderContext,
) {
    let (view_target, settings_index, settings, camera) = view.into_inner();
    if settings.dome_count == 0 { return; }

    // The post-process passes only run on HDR cameras (Rgba16Float framebuffer).
    assert!(
        camera.hdr,
        "inferno dome post-process requires an HDR camera; this PostProcessCamera lacks `Hdr`. \
         Add the `Hdr` component, or add a pipeline variant for its target format."
    );
    // Shaders compile asynchronously; skip while still compiling.
    let Some(pipeline) = pipeline_cache.get_render_pipeline(post_process_pipeline.pipeline_id) else { return; };
    let Some(settings_binding) = settings_uniforms.uniforms().binding() else { return; };
    let Some(storage_binding) = dome_storage.buffer.binding() else { return; };

    // Ping-pong: reads from `source`, writes to `destination`, then swaps for the next pass.
    let post_process = view_target.post_process_write();

    let bind_group = render_context.render_device().create_bind_group(
        "inferno_dome_post_process_bind_group",
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
            label: Some("inferno_dome_post_process_pass"),
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
struct InfernoDomePostProcessPipeline {
    layout: BindGroupLayoutDescriptor,
    sampler: Sampler,
    pipeline_id: CachedRenderPipelineId,
}

fn init_inferno_dome_pipeline(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    fullscreen_shader: Res<FullscreenShader>,
    pipeline_cache: Res<PipelineCache>,
    render_device: Res<RenderDevice>,
) {
    let layout = BindGroupLayoutDescriptor::new(
        "inferno_dome_post_process_bind_group_layout",
        &BindGroupLayoutEntries::sequential(
            ShaderStages::FRAGMENT,
            (
                texture_2d(TextureSampleType::Float { filterable: true }),
                sampler(SamplerBindingType::Filtering),
                // `true` = dynamic offset (per-camera slice in the shared uniform buffer).
                uniform_buffer::<InfernoDomePostProcess>(true),
                storage_buffer_read_only::<GpuInfernoDomeEntry>(false),
            ),
        ),
    );

    // Sub-pixel UV offsets require linear filtering.
    let sampler = render_device.create_sampler(&SamplerDescriptor {
        mag_filter: FilterMode::Linear,
        min_filter: FilterMode::Linear,
        ..default()
    });
    let shader = asset_server.load("shaders/weaponry/inferno_dome_post_process.wesl");

    let pipeline_id = pipeline_cache.queue_render_pipeline(RenderPipelineDescriptor {
        label: Some("inferno_dome_post_process_pipeline".into()),
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

    commands.insert_resource(InfernoDomePostProcessPipeline {
        layout,
        sampler,
        pipeline_id,
    });
}
