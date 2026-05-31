//! Post-process quantum field anomaly visualization.
//!
//! Each camera with [`QuantumFieldPostProcess`] gets a fullscreen pass that renders the
//! quantum field effect over the already-rendered frame (so it can distort / ghost the
//! walls, wisps and towers on top of a field). Two shared GPU storage buffers feed it:
//! one with the active field rectangles, one with the active scan-beam "collapse points"
//! (where drones are observing the field). Each camera keeps a slim uniform with projection
//! parameters, global time, and the current field count.
//!
//! Mirrors `weaponry/force_field_post_process.rs`. Differences: fields are axis-aligned
//! rectangles (located in-shader via a box SDF), they never overlap, intensity is driven by
//! `solve_progress` derived from `QuantumFieldLayers`, and drone scan beams collapse the
//! superposition locally around their ground spot. Pass ordering is defined centrally in
//! `lib_core::post_processing::PostProcessOrderingPlugin`.

use bevy::{
    core_pipeline::{
        core_2d::graph::Core2d,
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
            NodeRunError, RenderGraphContext, RenderGraphExt, ViewNode, ViewNodeRunner,
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

use lib_core::post_processing::QuantumFieldPostProcessLabel;
use crate::map_objects::quantum_field::QuantumFieldLayers;
use crate::units::expedition_drone::{ExpeditionDrone, ScanSpot, ScanningBeam};
use crate::prelude::*;

pub struct QuantumFieldPostProcessPlugin;
impl Plugin for QuantumFieldPostProcessPlugin {
    fn build(&self, app: &mut App) {
        app
            .add_plugins((
                ExtractComponentPlugin::<QuantumFieldPostProcess>::default(),
                UniformComponentPlugin::<QuantumFieldPostProcess>::default(),
                ExtractResourcePlugin::<QuantumFieldEntries>::default(),
                ExtractResourcePlugin::<CollapsePoints>::default(),
            ))
            .init_resource::<QuantumFieldEntries>()
            .init_resource::<CollapsePoints>()
            .add_observer(QuantumFieldPostProcess::on_add_post_process_camera)
            // `gather` runs before `update` so the per-camera `collapse_count` matches the points
            // uploaded this frame. Both ungated (run every frame) — empty queries are cheap.
            .add_systems(Update, (
                gather_collapse_points,
                QuantumFieldPostProcess::update,
            ).chain())
            ;

        let Some(render_app) = app.get_sub_app_mut(RenderApp) else { return; };
        render_app
            .init_resource::<GpuQuantumFieldStorage>()
            .init_resource::<GpuCollapsePointStorage>()
            .add_systems(RenderStartup, init_quantum_field_pipeline)
            .add_systems(Render, (
                GpuQuantumFieldStorage::prepare.in_set(RenderSystems::PrepareResources),
                GpuCollapsePointStorage::prepare.in_set(RenderSystems::PrepareResources),
            ))
            // Ordering against the other post-process passes lives in lib-core's
            // PostProcessOrderingPlugin (added after all effect plugins).
            .add_render_graph_node::<ViewNodeRunner<QuantumFieldPostProcessNode>>(
                Core2d,
                QuantumFieldPostProcessLabel,
            );
    }
}

// ── GPU data ──────────────────────────────────────────────────────────────────────────

/// Per-field entry uploaded to the shared storage buffer. 24 bytes, 8-aligned, no `vec3`.
#[derive(Clone, Copy, ShaderType, Default)]
struct GpuQuantumFieldEntry {
    center: Vec2,
    half_extent: Vec2,
    /// 0 = fresh anomaly, 1 = solved. Diminishes weirdness in the shader.
    solve_progress: f32,
    /// Per-field noise offset (derived from entity index).
    seed: f32,
}

#[derive(ShaderType, Default)]
struct GpuQuantumFieldBuffer {
    #[shader(size(runtime))]
    entries: Vec<GpuQuantumFieldEntry>,
}

/// Main-world resource holding this frame's field entries (cloned into render world each frame).
#[derive(Resource, Default, Clone)]
struct QuantumFieldEntries(Vec<GpuQuantumFieldEntry>);
impl ExtractResource for QuantumFieldEntries {
    type Source = QuantumFieldEntries;
    fn extract_resource(source: &Self::Source) -> Self { source.clone() }
}

/// One active scan-beam collapse point: where a drone's beam observes the field.
#[derive(Clone, Copy, ShaderType, Default)]
struct GpuCollapsePoint {
    /// Ground position of the scan spot, world space.
    pos: Vec2,
}

#[derive(ShaderType, Default)]
struct GpuCollapsePointBuffer {
    #[shader(size(runtime))]
    entries: Vec<GpuCollapsePoint>,
}

/// Main-world resource holding this frame's active collapse points.
#[derive(Resource, Default, Clone)]
struct CollapsePoints(Vec<GpuCollapsePoint>);
impl ExtractResource for CollapsePoints {
    type Source = CollapsePoints;
    fn extract_resource(source: &Self::Source) -> Self { source.clone() }
}

/// GPU-side storage buffer holding all active quantum field entries.
#[derive(Resource, Default)]
struct GpuQuantumFieldStorage {
    buffer: StorageBuffer<GpuQuantumFieldBuffer>,
}
impl GpuQuantumFieldStorage {
    fn prepare(
        mut extracted: ResMut<QuantumFieldEntries>,
        render_device: Res<RenderDevice>,
        render_queue: Res<RenderQueue>,
        mut storage: ResMut<GpuQuantumFieldStorage>,
    ) {
        let mut entries = std::mem::take(&mut extracted.0);
        // Some GPU backends reject 0-byte storage buffers; keep at least one dummy entry.
        // The shader early-returns when field_count == 0.
        if entries.is_empty() {
            entries.push(GpuQuantumFieldEntry::default());
        }
        storage.buffer.set(GpuQuantumFieldBuffer { entries });
        storage.buffer.write_buffer(&render_device, &render_queue);
    }
}

/// GPU-side storage buffer holding all active collapse points.
#[derive(Resource, Default)]
struct GpuCollapsePointStorage {
    buffer: StorageBuffer<GpuCollapsePointBuffer>,
}
impl GpuCollapsePointStorage {
    fn prepare(
        mut extracted: ResMut<CollapsePoints>,
        render_device: Res<RenderDevice>,
        render_queue: Res<RenderQueue>,
        mut storage: ResMut<GpuCollapsePointStorage>,
    ) {
        let mut entries = std::mem::take(&mut extracted.0);
        // Keep at least one entry so the buffer is never 0-byte. It is never read: with no active
        // points, `collapse_count` is 0 and the shader's loop doesn't run.
        if entries.is_empty() {
            entries.push(GpuCollapsePoint::default());
        }
        storage.buffer.set(GpuCollapsePointBuffer { entries });
        storage.buffer.write_buffer(&render_device, &render_queue);
    }
}

/// Per-camera uniform: projection parameters, global time, and the field / collapse-point counts.
#[derive(Component, ExtractComponent, Clone, Copy, ShaderType, Default)]
pub struct QuantumFieldPostProcess {
    camera_world_pos: Vec2,
    viewport_world_size: Vec2,
    global_time: f32,
    field_count: u32,
    /// Active scan-collapse points this frame. The shader loops on this rather than
    /// `arrayLength()` because the storage buffer grows but never shrinks (stale tail otherwise).
    collapse_count: u32,
}
impl QuantumFieldPostProcess {
    fn on_add_post_process_camera(
        trigger: On<Add, lib_core::camera::PostProcessCamera>,
        mut commands: Commands,
    ) {
        commands.entity(trigger.entity).insert(QuantumFieldPostProcess::default());
    }

    fn update(
        time: Res<Time>,
        mut entries: ResMut<QuantumFieldEntries>,
        collapse_points: Res<CollapsePoints>,
        fields: Query<(Entity, &QuantumFieldLayers, &Transform, &GridImprint)>,
        mut cameras: Query<(&mut QuantumFieldPostProcess, &Transform, &Projection)>,
    ) {
        entries.0.clear();
        for (entity, layers, transform, imprint) in fields.iter() {
            entries.0.push(GpuQuantumFieldEntry {
                center: transform.translation.xy(),
                half_extent: imprint.world_size() * 0.5,
                solve_progress: solve_progress(layers),
                // Stable per-field offset; decorrelates noise between fields.
                seed: entity.index_u32() as f32 * 0.37,
            });
        }

        let count = entries.0.len() as u32;
        let collapse_count = collapse_points.0.len() as u32;
        let global_time = time.elapsed_secs();
        for (mut post_process, transform, projection) in cameras.iter_mut() {
            let Projection::Orthographic(ortho) = &*projection else { continue; };
            post_process.camera_world_pos = transform.translation.xy();
            post_process.viewport_world_size = Vec2::new(ortho.area.width(), ortho.area.height());
            post_process.global_time = global_time;
            post_process.field_count = count;
            post_process.collapse_count = collapse_count;
        }
    }
}

/// Single 0→1 "tamed" scalar across all layers. 1.0 once the field is solved.
fn solve_progress(layers: &QuantumFieldLayers) -> f32 {
    let total = layers.layers.len().max(1) as f32;
    let partial = if layers.is_solved() {
        0.0
    } else {
        let target = layers.layers[layers.current_layer].value;
        if target > 0.0 { layers.current_layer_progress / target } else { 0.0 }
    };
    ((layers.current_layer as f32 + partial) / total).clamp(0.0, 1.0)
}

/// Collects the ground spot of every drone whose scan beam is currently active into the shared
/// `CollapsePoints` buffer. The shader calms the field locally around each (the "observation
/// collapses the wavefunction" effect). Spots always sit inside their target field, so no
/// field association is needed — the shader only processes field interiors anyway.
///
/// Each spot collapses at full strength. To soften the on/off pop as a beam toggles during
/// patrol, add a per-point intensity field here and multiply it into `collapse` in the shader.
fn gather_collapse_points(
    mut points: ResMut<CollapsePoints>,
    beams: Query<&ScanningBeam>,
    drones: Query<&ExpeditionDrone>,
    spots: Query<&Transform, With<ScanSpot>>,
) {
    points.0.clear();
    for beam in beams.iter() {
        let Ok(drone) = drones.get(beam.drone) else { continue; };
        if !drone.is_beam_active { continue; }
        let Ok(spot_transform) = spots.get(beam.spot) else { continue; };
        points.0.push(GpuCollapsePoint { pos: spot_transform.translation.xy() });
    }
}

// ── Render graph ──────────────────────────────────────────────────────────────────────

#[derive(Default)]
struct QuantumFieldPostProcessNode;
impl ViewNode for QuantumFieldPostProcessNode {
    type ViewQuery = (
        &'static ViewTarget,
        &'static DynamicUniformIndex<QuantumFieldPostProcess>,
        &'static QuantumFieldPostProcess,
    );

    fn run<'w>(
        &self,
        _graph: &mut RenderGraphContext,
        render_context: &mut RenderContext<'w>,
        (view_target, settings_index, settings): QueryItem<'w, 'w, Self::ViewQuery>,
        world: &'w World,
    ) -> Result<(), NodeRunError> {
        if settings.field_count == 0 {
            return Ok(());
        }
        let pipeline_res = world.resource::<QuantumFieldPostProcessPipeline>();
        let pipeline_cache = world.resource::<PipelineCache>();

        let pipeline_id = if view_target.main_texture_format() == ViewTarget::TEXTURE_FORMAT_HDR {
            pipeline_res.pipeline_id_hdr
        } else {
            pipeline_res.pipeline_id_ldr
        };
        let Some(pipeline) = pipeline_cache.get_render_pipeline(pipeline_id) else {
            return Ok(());
        };

        let settings_uniforms = world.resource::<ComponentUniforms<QuantumFieldPostProcess>>();
        let Some(settings_binding) = settings_uniforms.uniforms().binding() else {
            return Ok(());
        };

        let field_storage = world.resource::<GpuQuantumFieldStorage>();
        let Some(field_binding) = field_storage.buffer.binding() else {
            return Ok(());
        };

        let collapse_storage = world.resource::<GpuCollapsePointStorage>();
        let Some(collapse_binding) = collapse_storage.buffer.binding() else {
            return Ok(());
        };

        let post_process = view_target.post_process_write();

        let bind_group = render_context.render_device().create_bind_group(
            "quantum_field_post_process_bind_group",
            &pipeline_cache.get_bind_group_layout(&pipeline_res.layout),
            &BindGroupEntries::sequential((
                post_process.source,
                &pipeline_res.sampler,
                settings_binding.clone(),
                field_binding.clone(),
                collapse_binding.clone(),
            )),
        );

        let mut render_pass = render_context.begin_tracked_render_pass(RenderPassDescriptor {
            label: Some("quantum_field_post_process_pass"),
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
struct QuantumFieldPostProcessPipeline {
    layout: BindGroupLayoutDescriptor,
    sampler: Sampler,
    pipeline_id_ldr: CachedRenderPipelineId,
    pipeline_id_hdr: CachedRenderPipelineId,
}

fn init_quantum_field_pipeline(
    mut commands: Commands,
    render_device: Res<RenderDevice>,
    asset_server: Res<AssetServer>,
    fullscreen_shader: Res<FullscreenShader>,
    pipeline_cache: Res<PipelineCache>,
) {
    let layout = BindGroupLayoutDescriptor::new(
        "quantum_field_post_process_bind_group_layout",
        &BindGroupLayoutEntries::sequential(
            ShaderStages::FRAGMENT,
            (
                texture_2d(TextureSampleType::Float { filterable: true }),
                sampler(SamplerBindingType::Filtering),
                uniform_buffer::<QuantumFieldPostProcess>(true),
                storage_buffer_read_only::<GpuQuantumFieldEntry>(false),
                storage_buffer_read_only::<GpuCollapsePoint>(false),
            ),
        ),
    );

    let sampler = render_device.create_sampler(&SamplerDescriptor::default());
    let shader = asset_server.load("shaders/quantum_field_post_process.wgsl");

    let make_pipeline = |format| RenderPipelineDescriptor {
        label: Some("quantum_field_post_process_pipeline".into()),
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

    commands.insert_resource(QuantumFieldPostProcessPipeline {
        layout,
        sampler,
        pipeline_id_ldr,
        pipeline_id_hdr,
    });
}
