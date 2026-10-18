//! # Cut lane
//!
//! Draws `DISPLACE` and `OVERLAY` parts before their band, using instanced quads.
//! Each kind uses one draw per layer and part:
//!
//! - **Displacement** adds offsets and brightness gains to a shared buffer. A full-screen pass
//!   applies the combined result to the scene from lower bands.
//! - **Overlay** draws over that scene and can sample its snapshot through the shared GPU group.
//!   Overlays draw in layer order within each band.
//!
//! Cuts run before bloom and tonemapping, preserving HDR colour for those passes.
//!
//! Each frame, the lane gathers entities with the kind's component, `GlobalTransform` and `ZDepth`.
//! It skips entities with a false `ViewVisibility` and includes entities without that component.
//! Each quad uses `GlobalTransform` and `cut_quad_size`. The instance binding covers exactly the
//! gathered list: `arrayLength` gives its length, and `instance_index` gives an entry's position.
//!
//! ## Displacement buffer channels
//!
//! - `rg`: offset, in UV units, from the pixel to where the scene is sampled.
//! - `b`: brightness gain; the scene colour is multiplied by `1 + b`.
//! - `a`: unused.

use core::{marker::PhantomData, num::NonZeroU64, ops::Range};

use bevy::{
    camera::visibility::VisibilitySystems,
    prelude::*,
    render::{
        Render, RenderApp, RenderStartup, RenderSystems,
        extract_resource::{ExtractResource, ExtractResourcePlugin},
        globals::GlobalsUniform,
        render_resource::{binding_types::{storage_buffer_read_only, uniform_buffer}, *},
        renderer::{RenderDevice, RenderQueue},
        view::ViewUniform,
    },
    shader::Shader,
};

use game_core::z_depth::{Band, PerBand, ZDepth};

use crate::{lane::{LaneParts, LaneVisual, entry_shader_id, look_asset_path}, shader::ADDITIVE_BLENDING, shared_gpu::add_shared_gpu_layout};

/// Distinguishes the cut lane's entry shader ids from other lanes'.
const CUT_LANE: &str = "cut_lane";

/// Adds the `DISPLACE` and `OVERLAY` parts of kind `K`.
pub(crate) fn build<K: LaneVisual>(app: &mut App) {
    app
        .init_resource::<CutLaneInstances<K>>()
        .add_plugins(ExtractResourcePlugin::<CutLaneInstances<K>>::default())
        .add_systems(Startup, init_cut_entry_shader::<K>)
        .add_systems(PostUpdate, gather_cut_instances::<K>.after(VisibilitySystems::CheckVisibility));

    let Some(render_app) = app.get_sub_app_mut(RenderApp) else { return; };
    render_app
        .init_resource::<CutLaneBuffers<K>>()
        .add_systems(RenderStartup, init_cut_lane_pipelines::<K>)
        .add_systems(Render, prepare_cut_lane::<K>.in_set(RenderSystems::PrepareResources).in_set(CutPrepareSystems));
}

/// Format of a band's displacement buffer.
pub const DISPLACEMENT_FORMAT: TextureFormat = TextureFormat::Rgba16Float;
/// Format of the scene the cut works on: the target of an HDR camera.
pub const SCENE_FORMAT: TextureFormat = TextureFormat::Rgba16Float;

/// One kind's instanced draw for one layer and part.
pub struct CutBatch {
    pub layer: ZDepth,
    pub label: &'static str,
    pub instances: Range<u32>,
    pub pipeline: CachedRenderPipelineId,
    /// Group 1: the instances' quads.
    pub quad_bind_group: BindGroup,
    /// Group 2: the instances.
    pub instance_bind_group: BindGroup,
}

/// Cut draws submitted by all visual kinds for this frame, grouped by band and part.
#[derive(Resource, Default)]
pub struct CutBatches {
    displacement: PerBand<Vec<CutBatch>>,
    overlay: PerBand<Vec<CutBatch>>,
}
impl CutBatches {
    pub fn displacement_of(&self, band: Band) -> &[CutBatch] {
        &self.displacement[band]
    }

    /// Overlays of `band`, in draw order.
    pub fn overlay_of(&self, band: Band) -> &[CutBatch] {
        &self.overlay[band]
    }

    pub fn cuts(&self, band: Band) -> bool {
        !self.displacement[band].is_empty() || !self.overlay[band].is_empty()
    }

    /// Sorts each band's overlays by layer, then by kind label.
    pub fn sort_overlays(&mut self) {
        for overlays in self.overlay.iter_mut() {
            overlays.sort_by_key(|batch| (batch.layer, batch.label));
        }
    }

    pub fn clear(&mut self) {
        self.displacement.iter_mut().chain(self.overlay.iter_mut()).for_each(Vec::clear);
    }
}

/// System set for preparing each kind's buffers and submitting draws to [`CutBatches`].
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct CutPrepareSystems;

/// Group 0 layout for cut pipelines: a dynamically indexed view uniform and Bevy's globals.
/// Bindings match `bevy_sprite_render::mesh2d::view_bindings` so look modules can use those imports.
pub fn cut_view_layout() -> BindGroupLayoutDescriptor {
    BindGroupLayoutDescriptor::new(
        "cut_view_layout",
        &BindGroupLayoutEntries::sequential(
            ShaderStages::VERTEX_FRAGMENT,
            (uniform_buffer::<ViewUniform>(true), uniform_buffer::<GlobalsUniform>(false)),
        ),
    )
}

/// An instance's quad in world space: its two edge vectors and its centre. Field order and types
/// mirror `LaneQuad` in `assets/shaders/common/lane_quad.wesl`.
#[derive(ShaderType, Clone, Copy)]
struct LaneQuad {
    x_axis: Vec2,
    y_axis: Vec2,
    center: Vec2,
}

/// This frame's instances of kind `K`, each with its layer and quad, sorted by layer.
#[derive(Resource, ExtractResource, Clone)]
#[extract_app(RenderApp)]
struct CutLaneInstances<K: LaneVisual> {
    entries: Vec<(ZDepth, LaneQuad, K)>,
}
impl<K: LaneVisual> Default for CutLaneInstances<K> {
    fn default() -> Self {
        Self { entries: Vec::new() }
    }
}

/// Keeps the modules the generated entry shader imports loaded.
#[derive(Resource)]
struct CutLaneLook<K: LaneVisual> {
    _look: Handle<Shader>,
    _lane_quad: Handle<Shader>,
    kind: PhantomData<K>,
}

fn cut_entry_shader_source<K: LaneVisual>() -> String {
    let displaces = K::PARTS.has(LaneParts::DISPLACE);
    let overlays = K::PARTS.has(LaneParts::OVERLAY);
    let mut imports = vec!["Instance"];
    if K::READS_ALL_INSTANCES { imports.push("instances"); }
    if displaces { imports.push("displace"); }
    if overlays { imports.push("overlay"); }

    let mut source = format!(
        "import bevy_sprite_render::mesh2d::vertex_output::VertexOutput;
import package::shaders::common::lane_quad::{{LaneQuad, lane_quad_vertex}};
import package::{look_module}::{{{imports}}};

@group(1) @binding(0) var<storage, read> quads: array<LaneQuad>;
",
        look_module = K::LOOK,
        imports = imports.join(", "),
    );
    if !K::READS_ALL_INSTANCES {
        source.push_str("@group(2) @binding(0) var<storage, read> instances: array<Instance>;\n");
    }
    source.push_str(
        "
@vertex
fn vertex(@builtin(vertex_index) vertex_index: u32, @builtin(instance_index) instance_index: u32) -> VertexOutput {
    return lane_quad_vertex(quads[instance_index], vertex_index, instance_index);
}
",
    );
    if displaces {
        source.push_str(
            "
@fragment
fn lane_displacement(in: VertexOutput) -> @location(0) vec4<f32> {
    return displace(in, instances[in.instance_index]);
}
",
        );
    }
    if overlays {
        source.push_str(
            "
@fragment
fn lane_overlay(in: VertexOutput) -> @location(0) vec4<f32> {
    return overlay(in, instances[in.instance_index]);
}
",
        );
    }
    source
}

fn init_cut_entry_shader<K: LaneVisual>(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    mut shaders: ResMut<Assets<Shader>>,
) {
    shaders
        .insert(&entry_shader_id::<K>(CUT_LANE), Shader::from_wesl(cut_entry_shader_source::<K>(), format!("shaders/lanes/{}_cut.wesl", K::LABEL)))
        .expect("a uuid shader id is always valid");
    commands.insert_resource(CutLaneLook::<K> {
        _look: asset_server.load(look_asset_path::<K>()),
        _lane_quad: asset_server.load("shaders/common/lane_quad.wesl"),
        kind: PhantomData,
    });
}

fn gather_cut_instances<K: LaneVisual>(
    mut instances: ResMut<CutLaneInstances<K>>,
    visuals: Query<(&K, &GlobalTransform, &ZDepth, Option<&ViewVisibility>)>,
) {
    instances.entries.clear();
    for (visual, transform, layer, visibility) in visuals.iter() {
        if visibility.is_some_and(|visibility| !visibility.get()) { continue; }
        let size = visual.cut_quad_size();
        let affine = transform.affine();
        let quad = LaneQuad {
            x_axis: (affine.matrix3.x_axis * size.x).truncate(),
            y_axis: (affine.matrix3.y_axis * size.y).truncate(),
            center: affine.translation.truncate(),
        };
        instances.entries.push((*layer, quad, visual.clone()));
    }
    instances.entries.sort_by_key(|(layer, _, _)| *layer);
}

#[derive(Resource)]
struct CutLaneBuffers<K: LaneVisual> {
    quads: BufferVec<LaneQuad>,
    instances: BufferVec<K>,
}
impl<K: LaneVisual> Default for CutLaneBuffers<K> {
    fn default() -> Self {
        Self { quads: BufferVec::new(BufferUsages::STORAGE), instances: BufferVec::new(BufferUsages::STORAGE) }
    }
}

#[derive(Resource)]
struct CutLanePipelines<K: LaneVisual> {
    quad_layout: BindGroupLayoutDescriptor,
    instance_layout: BindGroupLayoutDescriptor,
    displacement: Option<CachedRenderPipelineId>,
    overlay: Option<CachedRenderPipelineId>,
    kind: PhantomData<K>,
}

fn init_cut_lane_pipelines<K: LaneVisual>(
    mut commands: Commands,
    pipeline_cache: Res<PipelineCache>,
) {
    let quad_layout = BindGroupLayoutDescriptor::new(
        "lane_quad_layout",
        &BindGroupLayoutEntries::single(ShaderStages::VERTEX_FRAGMENT, storage_buffer_read_only::<LaneQuad>(false)),
    );
    let instance_layout = BindGroupLayoutDescriptor::new(
        K::LABEL,
        &BindGroupLayoutEntries::single(ShaderStages::VERTEX_FRAGMENT, storage_buffer_read_only::<K>(false)),
    );
    let shader = entry_shader_id::<K>(CUT_LANE);
    let pipeline = |entry_point: &'static str, format: TextureFormat, blend: BlendState| {
        let mut descriptor = RenderPipelineDescriptor {
            label: Some(format!("{}_{entry_point}", K::LABEL).into()),
            layout: vec![cut_view_layout(), quad_layout.clone(), instance_layout.clone()],
            vertex: VertexState { shader: shader.clone(), entry_point: Some("vertex".into()), ..default() },
            fragment: Some(FragmentState {
                shader: shader.clone(),
                entry_point: Some(entry_point.into()),
                targets: vec![Some(ColorTargetState { format, blend: Some(blend), write_mask: ColorWrites::ALL })],
                ..default()
            }),
            ..default()
        };
        add_shared_gpu_layout(&mut descriptor);
        pipeline_cache.queue_render_pipeline(descriptor)
    };
    let displacement = K::PARTS.has(LaneParts::DISPLACE)
        .then(|| pipeline("lane_displacement", DISPLACEMENT_FORMAT, ADDITIVE_BLENDING));
    let overlay = K::PARTS.has(LaneParts::OVERLAY)
        .then(|| pipeline("lane_overlay", SCENE_FORMAT, K::overlay_blend()));
    commands.insert_resource(CutLanePipelines::<K> { quad_layout, instance_layout, displacement, overlay, kind: PhantomData });
}

/// Binds only the first `count` elements so shader `arrayLength` excludes unused buffer capacity.
fn live_binding(buffer: &Buffer, element_size: NonZeroU64, count: usize) -> BindingResource<'_> {
    BindingResource::Buffer(BufferBinding {
        buffer,
        offset: 0,
        size: NonZeroU64::new(element_size.get() * count as u64),
    })
}

fn prepare_cut_lane<K: LaneVisual>(
    render_device: Res<RenderDevice>,
    render_queue: Res<RenderQueue>,
    pipeline_cache: Res<PipelineCache>,
    instances: Res<CutLaneInstances<K>>,
    pipelines: Res<CutLanePipelines<K>>,
    mut buffers: ResMut<CutLaneBuffers<K>>,
    mut batches: ResMut<CutBatches>,
) {
    if instances.entries.is_empty() { return; }

    buffers.quads.clear();
    buffers.instances.clear();
    for (_, quad, instance) in instances.entries.iter() {
        buffers.quads.push(*quad);
        buffers.instances.push(instance.clone());
    }
    buffers.quads.write_buffer(&render_device, &render_queue);
    buffers.instances.write_buffer(&render_device, &render_queue);
    let (Some(quad_buffer), Some(instance_buffer)) = (buffers.quads.buffer(), buffers.instances.buffer()) else { return; };
    let quad_bind_group = render_device.create_bind_group(
        "lane_quads",
        &pipeline_cache.get_bind_group_layout(&pipelines.quad_layout),
        &BindGroupEntries::single(live_binding(quad_buffer, LaneQuad::min_size(), instances.entries.len())),
    );
    let instance_bind_group = render_device.create_bind_group(
        K::LABEL,
        &pipeline_cache.get_bind_group_layout(&pipelines.instance_layout),
        &BindGroupEntries::single(live_binding(instance_buffer, K::min_size(), instances.entries.len())),
    );

    // Entries are sorted by layer, so each layer is one contiguous instance range.
    let mut start = 0;
    for run in instances.entries.chunk_by(|(layer, _, _), (other, _, _)| layer == other) {
        let layer = run[0].0;
        let end = start + run.len();
        let band = layer.band();
        let batch = |pipeline| CutBatch {
            layer,
            label: K::LABEL,
            instances: start as u32..end as u32,
            pipeline,
            quad_bind_group: quad_bind_group.clone(),
            instance_bind_group: instance_bind_group.clone(),
        };
        if let Some(pipeline) = pipelines.displacement {
            batches.displacement[band].push(batch(pipeline));
        }
        if let Some(pipeline) = pipelines.overlay {
            batches.overlay[band].push(batch(pipeline));
        }
        start = end;
    }
}
