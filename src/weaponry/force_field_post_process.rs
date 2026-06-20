//! Post-process force field dome visualization.
//!
//! Each camera with [`ForceFieldPostProcess`] gets a fullscreen Voronoi bubble pass that
//! runs after the ripple post-process pass. Field entries live in a shared GPU storage
//! buffer; each camera keeps a slim uniform with projection parameters, global time,
//! and the current field count.

use super::force_field::{ForceField, ForceFieldEntered};
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
use lib_core::camera::MainCamera;
use lib_core::post_processing::ForceFieldPostProcessSet;
use std::{cmp::Ordering, collections::BinaryHeap};

pub struct ForceFieldPostProcessPlugin;
impl Plugin for ForceFieldPostProcessPlugin {
    fn build(&self, app: &mut App) {
        app
            .add_plugins((
                ExtractComponentPlugin::<ForceFieldPostProcess>::default(),
                UniformComponentPlugin::<ForceFieldPostProcess>::default(),
                ExtractResourcePlugin::<ForceFieldEntries>::default(),
                ExtractResourcePlugin::<FieldRippleEntries>::default(),
            ))
            .init_resource::<ForceFieldEntries>()
            .init_resource::<FieldRippleEntries>()
            .add_observer(ForceFieldPostProcess::on_add_post_process_camera)
            .add_observer(on_field_entered_create_ripple)
            .add_systems(Update, ForceFieldPostProcess::update)
            ;

        let Some(render_app) = app.get_sub_app_mut(RenderApp) else { return; };
        render_app
            .init_resource::<GpuForceFieldStorage>()
            .init_resource::<GpuFieldRippleStorage>()
            .add_systems(RenderStartup, init_force_field_pipeline)
            .add_systems(Render, (
                GpuForceFieldStorage::prepare.in_set(RenderSystems::PrepareResources),
                GpuFieldRippleStorage::prepare.in_set(RenderSystems::PrepareResources),
            ))
            // Ordering against the other post-process passes lives in lib-core's
            // PostProcessOrderingPlugin (added after all effect plugins).
            .add_systems(Core2d, force_field_post_process_pass.in_set(ForceFieldPostProcessSet));
    }
}

// ── GPU data ──────────────────────────────────────────────────────────────────────────

/// Per-field entry uploaded to the shared storage buffer.
#[derive(Clone, Copy, ShaderType, Default)]
struct GpuForceFieldEntry {
    center: Vec2,
    radius: f32,
    progress: f32,
    visual_noise_offset: f32,
}

#[derive(ShaderType, Default)]
struct GpuForceFieldBuffer {
    #[shader(size(runtime))]
    entries: Vec<GpuForceFieldEntry>,
}

/// Main-world resource holding this frame's field entries (cloned into render world each frame).
#[derive(Resource, Default, Clone)]
struct ForceFieldEntries(Vec<GpuForceFieldEntry>);
impl ExtractResource for ForceFieldEntries {
    type Source = ForceFieldEntries;
    fn extract_resource(source: &Self::Source) -> Self { source.clone() }
}

// ── Impact ripples ─────────────────────────────────────────────────────────────────────

const MAX_FIELD_RIPPLES: usize = 64;
// Must match RIPPLE_LIFETIME in the shader.
const FIELD_RIPPLE_LIFETIME: f32 = 3.0;

/// One impact ripple spawned when a wisp enters a force field dome.
/// Ordering is by `spawn_time` (oldest = greatest) so the struct can live directly
/// in a `BinaryHeap`: Rust's max-heap then puts the oldest entry at the top for
/// efficient ejection when the buffer is full.
#[derive(Clone, Copy, ShaderType)]
struct GpuFieldRipple {
    hit_pos:      Vec2,
    field_center: Vec2,
    spawn_time:   f32,
    _pad:         f32,
}
impl Default for GpuFieldRipple {
    fn default() -> Self {
        // spawn_time < 0 is the inactive sentinel used for empty-buffer padding.
        Self { hit_pos: Vec2::ZERO, field_center: Vec2::ZERO, spawn_time: -1.0, _pad: 0.0 }
    }
}
impl PartialEq for GpuFieldRipple {
    fn eq(&self, other: &Self) -> bool { self.spawn_time.to_bits() == other.spawn_time.to_bits() }
}
impl Eq for GpuFieldRipple {}
impl PartialOrd for GpuFieldRipple {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> { Some(self.cmp(other)) }
}
impl Ord for GpuFieldRipple {
    fn cmp(&self, other: &Self) -> Ordering {
        other.spawn_time.total_cmp(&self.spawn_time)
    }
}

#[derive(ShaderType, Default)]
struct GpuFieldRippleBuffer {
    #[shader(size(runtime))]
    entries: Vec<GpuFieldRipple>,
}

/// Min-heap (by spawn_time) of active impact ripples, capped at MAX_FIELD_RIPPLES.
/// Stale entries are ejected before each insert; when still at capacity the oldest
/// is bumped. Only ripples visible to the main camera are recorded.
#[derive(Resource, Clone, Default)]
struct FieldRippleEntries {
    heap: BinaryHeap<GpuFieldRipple>,
}
impl FieldRippleEntries {
    fn push(&mut self, ripple: GpuFieldRipple, current_time: f32) {
        while let Some(top) = self.heap.peek() {
            if current_time - top.spawn_time > FIELD_RIPPLE_LIFETIME {
                self.heap.pop();
            } else {
                break;
            }
        }
        if self.heap.len() >= MAX_FIELD_RIPPLES {
            self.heap.pop();
        }
        self.heap.push(ripple);
    }
}
impl ExtractResource for FieldRippleEntries {
    type Source = FieldRippleEntries;
    fn extract_resource(source: &Self::Source) -> Self { source.clone() }
}

/// GPU-side storage buffer holding all active force field entries.
#[derive(Resource, Default)]
struct GpuForceFieldStorage {
    buffer: StorageBuffer<GpuForceFieldBuffer>,
}
impl GpuForceFieldStorage {
    fn prepare(
        mut extracted: ResMut<ForceFieldEntries>,
        render_device: Res<RenderDevice>,
        render_queue: Res<RenderQueue>,
        mut storage: ResMut<GpuForceFieldStorage>,
    ) {
        let mut entries = std::mem::take(&mut extracted.0);
        if entries.is_empty() {
            entries.push(GpuForceFieldEntry::default());
        }
        storage.buffer.set(GpuForceFieldBuffer { entries });
        storage.buffer.write_buffer(&render_device, &render_queue);
    }
}

/// GPU-side storage buffer holding all active impact ripples.
#[derive(Resource, Default)]
struct GpuFieldRippleStorage {
    buffer: StorageBuffer<GpuFieldRippleBuffer>,
}
impl GpuFieldRippleStorage {
    fn prepare(
        mut extracted: ResMut<FieldRippleEntries>,
        render_device: Res<RenderDevice>,
        render_queue: Res<RenderQueue>,
        mut storage: ResMut<GpuFieldRippleStorage>,
    ) {
        let mut entries = std::mem::take(&mut extracted.heap).into_vec();
        if entries.is_empty() {
            entries.push(GpuFieldRipple::default());
        }
        storage.buffer.set(GpuFieldRippleBuffer { entries });
        storage.buffer.write_buffer(&render_device, &render_queue);
    }
}

fn on_field_entered_create_ripple(
    trigger: On<ForceFieldEntered>,
    time: Res<Time>,
    transforms: Query<&Transform>,
    camera: Single<(&Transform, &Projection), With<MainCamera>>,
    mut ripples: ResMut<FieldRippleEntries>,
) {
    if trigger.event().exited_field.is_some() { return; }
    let Ok(wisp_transform)  = transforms.get(trigger.event().target) else { return; };
    let Ok(field_transform) = transforms.get(trigger.event().field)  else { return; };

    let hit_pos      = wisp_transform.translation.xy();
    let field_center = field_transform.translation.xy();

    // Only record ripples for fields visible to the main camera.
    // Using field_center + generous margin so partially-off-screen domes aren't skipped.
    let (cam_transform, cam_projection) = *camera;
    let Projection::Orthographic(ortho) = cam_projection else { return; };
    let cam_pos  = cam_transform.translation.xy();
    let half     = Vec2::new(ortho.area.width(), ortho.area.height()) * 0.5;
    let margin   = Vec2::splat(200.0);
    if (field_center - cam_pos).abs().cmpgt(half + margin).any() { return; }

    let current_time = time.elapsed_secs();
    ripples.push(GpuFieldRipple {
        hit_pos,
        field_center,
        spawn_time: current_time,
        _pad: 0.0,
    }, current_time);
}

/// Per-camera uniform: projection parameters, global time, and field count.
#[derive(Component, ExtractComponent, Clone, Copy, ShaderType, Default)]
pub struct ForceFieldPostProcess {
    camera_world_pos: Vec2,
    viewport_world_size: Vec2,
    global_time: f32,
    field_count: u32,
}
impl ForceFieldPostProcess {
    fn on_add_post_process_camera(
        trigger: On<Add, lib_core::camera::PostProcessCamera>,
        mut commands: Commands,
    ) {
        commands.entity(trigger.entity).insert(ForceFieldPostProcess::default());
    }

    fn update(
        time: Res<Time>,
        mut entries: ResMut<ForceFieldEntries>,
        fields: Query<(&ForceField, &Transform)>,
        mut cameras: Query<(&mut ForceFieldPostProcess, &Transform, &Projection)>,
    ) {
        entries.0.clear();
        for (field, transform) in fields.iter() {
            entries.0.push(GpuForceFieldEntry {
                center: transform.translation.xy(),
                radius: field.radius,
                progress: field.progress,
                visual_noise_offset: field.visual_noise_offset,
            });
        }

        let count = entries.0.len() as u32;
        let global_time = time.elapsed_secs();
        for (mut post_process, transform, projection) in cameras.iter_mut() {
            let Projection::Orthographic(ortho) = &*projection else { continue; };
            post_process.camera_world_pos = transform.translation.xy();
            post_process.viewport_world_size = Vec2::new(ortho.area.width(), ortho.area.height());
            post_process.global_time = global_time;
            post_process.field_count = count;
        }
    }
}

// ── Render pass system ───────────────────────────────────────────────────────────────

fn force_field_post_process_pass(
    view: ViewQuery<(
        &ViewTarget,
        &DynamicUniformIndex<ForceFieldPostProcess>,
        &ForceFieldPostProcess,
        &ExtractedCamera,
    )>,
    pipeline_res: Res<ForceFieldPostProcessPipeline>,
    pipeline_cache: Res<PipelineCache>,
    settings_uniforms: Res<ComponentUniforms<ForceFieldPostProcess>>,
    field_storage: Res<GpuForceFieldStorage>,
    ripple_storage: Res<GpuFieldRippleStorage>,
    mut ctx: RenderContext,
) {
    let (view_target, settings_index, settings, camera) = view.into_inner();

    if settings.field_count == 0 {
        return;
    }

    // The post-process passes only run on HDR cameras (Rgba16Float framebuffer).
    assert!(
        camera.hdr,
        "force field post-process requires an HDR camera; this PostProcessCamera lacks `Hdr`. \
         Add the `Hdr` component, or add a pipeline variant for its target format."
    );
    let pipeline_id = pipeline_res.pipeline_id;
    let Some(pipeline) = pipeline_cache.get_render_pipeline(pipeline_id) else { return; };
    let Some(settings_binding) = settings_uniforms.uniforms().binding() else { return; };
    let Some(storage_binding) = field_storage.buffer.binding() else { return; };
    let Some(ripple_binding) = ripple_storage.buffer.binding() else { return; };

    let post_process = view_target.post_process_write();

    let bind_group = ctx.render_device().create_bind_group(
        "force_field_post_process_bind_group",
        &pipeline_cache.get_bind_group_layout(&pipeline_res.layout),
        &BindGroupEntries::sequential((
            post_process.source,
            &pipeline_res.sampler,
            settings_binding.clone(),
            storage_binding.clone(),
            ripple_binding.clone(),
        )),
    );

    let mut render_pass = ctx
        .command_encoder()
        .begin_render_pass(&RenderPassDescriptor {
            label: Some("force_field_post_process_pass"),
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
    render_pass.set_bind_group(0, &bind_group, &[settings_index.index()]);
    render_pass.draw(0..3, 0..1);
}

// ── Pipeline ──────────────────────────────────────────────────────────────────────────

#[derive(Resource)]
struct ForceFieldPostProcessPipeline {
    layout: BindGroupLayoutDescriptor,
    sampler: Sampler,
    pipeline_id: CachedRenderPipelineId,
}

fn init_force_field_pipeline(
    mut commands: Commands,
    render_device: Res<RenderDevice>,
    asset_server: Res<AssetServer>,
    fullscreen_shader: Res<FullscreenShader>,
    pipeline_cache: Res<PipelineCache>,
) {
    let layout = BindGroupLayoutDescriptor::new(
        "force_field_post_process_bind_group_layout",
        &BindGroupLayoutEntries::sequential(
            ShaderStages::FRAGMENT,
            (
                texture_2d(TextureSampleType::Float { filterable: true }),
                sampler(SamplerBindingType::Filtering),
                uniform_buffer::<ForceFieldPostProcess>(true),
                storage_buffer_read_only::<GpuForceFieldEntry>(false),
                storage_buffer_read_only::<GpuFieldRipple>(false),
            ),
        ),
    );

    let sampler = render_device.create_sampler(&SamplerDescriptor::default());
    let shader = asset_server.load("shaders/force_field_post_process.wgsl");

    let make_pipeline = |format| RenderPipelineDescriptor {
        label: Some("force_field_post_process_pipeline".into()),
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

    commands.insert_resource(ForceFieldPostProcessPipeline {
        layout,
        sampler,
        pipeline_id,
    });
}
