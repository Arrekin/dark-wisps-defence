//! Post-process quantum field anomaly visualization.
//!
//! Each camera with [`QuantumFieldPostProcess`] gets a fullscreen pass that renders the
//! quantum field effect over the already-rendered frame (so it can distort / ghost the
//! walls, wisps and towers on top of a field). Field entries live in a shared GPU storage
//! buffer; each camera keeps a slim uniform with projection parameters, global time, and
//! the current field count.
//!
//! Mirrors `weaponry/force_field_post_process.rs`. Differences: fields are axis-aligned
//! rectangles (located in-shader via a box SDF), they never overlap, and intensity is driven
//! by `solve_progress` derived from `QuantumFieldLayers`. The pass runs after the ripple pass
//! and before the force field pass so a dome composites on top of a ground anomaly — the
//! ordering is defined centrally in `lib_core::post_processing::PostProcessOrderingPlugin`.

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
use crate::prelude::*;

pub struct QuantumFieldPostProcessPlugin;
impl Plugin for QuantumFieldPostProcessPlugin {
    fn build(&self, app: &mut App) {
        app
            .add_plugins((
                ExtractComponentPlugin::<QuantumFieldPostProcess>::default(),
                UniformComponentPlugin::<QuantumFieldPostProcess>::default(),
                ExtractResourcePlugin::<QuantumFieldEntries>::default(),
            ))
            .init_resource::<QuantumFieldEntries>()
            .add_observer(QuantumFieldPostProcess::on_add_post_process_camera)
            .add_systems(Update, QuantumFieldPostProcess::update)
            ;

        let Some(render_app) = app.get_sub_app_mut(RenderApp) else { return; };
        render_app
            .init_resource::<GpuQuantumFieldStorage>()
            .add_systems(RenderStartup, init_quantum_field_pipeline)
            .add_systems(Render, GpuQuantumFieldStorage::prepare.in_set(RenderSystems::PrepareResources))
            // Ordering against the other post-process passes lives in lib-core's
            // PostProcessOrderingPlugin (added after all effect plugins).
            .add_render_graph_node::<ViewNodeRunner<QuantumFieldPostProcessNode>>(
                Core2d,
                QuantumFieldPostProcessLabel,
            );
    }
}

// ── GPU data ──────────────────────────────────────────────────────────────────────────

/// Per-field entry uploaded to the shared storage buffer. 32 bytes, 8-aligned, no `vec3`.
#[derive(Clone, Copy, ShaderType, Default)]
struct GpuQuantumFieldEntry {
    center: Vec2,
    half_extent: Vec2,
    /// 0 = fresh anomaly, 1 = solved. Diminishes weirdness in the shader.
    solve_progress: f32,
    /// Per-field noise offset (derived from entity index).
    seed: f32,
    /// RESERVED for future sweep-collapse; always 0.0 for now (see tasks.md §6).
    scan_activity: f32,
    _pad: f32,
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

/// Per-camera uniform: projection parameters, global time, and field count.
#[derive(Component, ExtractComponent, Clone, Copy, ShaderType, Default)]
pub struct QuantumFieldPostProcess {
    camera_world_pos: Vec2,
    viewport_world_size: Vec2,
    global_time: f32,
    field_count: u32,
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
                scan_activity: 0.0,
                _pad: 0.0,
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
        let Some(storage_binding) = field_storage.buffer.binding() else {
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
                storage_binding.clone(),
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
