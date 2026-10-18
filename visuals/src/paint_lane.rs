//! # Paint lane
//!
//! Draws `PAINT` instances with a shared mesh and `Material2d` asset per kind. Bevy can batch
//! adjacent instances in the sorted transparent phase. Each entity receives a `MeshTag` containing
//! its instance-buffer slot. The generated shader reads `instances[get_tag(instance_index)]`
//! and passes that instance to the look module's `look` function.
//!
//! ## Slots
//!
//! Instances keep their slots until removal, when the slots return to a free list. The buffer
//! never compacts and grows to the kind's peak instance count. Freed slots retain stale data,
//! so paint shaders must read only their own slot.

use core::marker::PhantomData;

use bevy::{
    asset::RenderAssetUsages,
    camera::visibility::VisibilitySystems,
    mesh::{MeshTag, MeshVertexBufferLayoutRef},
    prelude::*,
    reflect::TypePath,
    render::{
        render_resource::{AsBindGroup, RenderPipelineDescriptor, SpecializedMeshPipelineError, encase::StorageBuffer},
        storage::ShaderBuffer,
    },
    shader::{Shader, ShaderRef},
    sprite_render::{AlphaMode2d, ExtendedMaterial2d, Material2d, Material2dKey, Material2dPipeline, Material2dPlugin},
};

use game_core::z_depth::ZDepth;

use crate::{
    lane::{BandsReadingBeneath, LaneMaterial, LaneMaterialHandle, LaneVisual, entry_shader_id, look_asset_path},
    shader::set_blend,
    shared_gpu::add_shared_gpu_layout,
};

/// Distinguishes the paint lane's entry shader ids from other lanes'.
const PAINT_LANE: &str = "paint_lane";

/// Adds the `PAINT` part of kind `K`.
pub(crate) fn build<K: LaneVisual>(app: &mut App) {
    app
        .add_plugins(Material2dPlugin::<LaneMaterial<K>>::default())
        .add_systems(Startup, init_paint_lane::<K>)
        .add_systems(Last, upload_paint_instances::<K>)
        .add_observer(on_add_assign_lane_slot::<K>)
        .add_observer(on_remove_free_lane_slot::<K>);
    if K::READS_BENEATH {
        app.add_systems(PostUpdate, mark_bands_reading_beneath::<K>.after(VisibilitySystems::CheckVisibility));
    }
}

/// Instance-buffer binding used by the generated paint shader.
#[derive(Asset, AsBindGroup, TypePath, Clone)]
pub struct PaintMaterial<K: LaneVisual> {
    #[storage(0, read_only)]
    instances: Handle<ShaderBuffer>,
    kind: PhantomData<K>,
}
impl<K: LaneVisual> Material2d for PaintMaterial<K> {
    fn fragment_shader() -> ShaderRef {
        ShaderRef::Handle(entry_shader_id::<K>(PAINT_LANE))
    }

    fn alpha_mode(&self) -> AlphaMode2d {
        AlphaMode2d::Blend
    }

    fn specialize(
        _pipeline: &Material2dPipeline,
        descriptor: &mut RenderPipelineDescriptor,
        _layout: &MeshVertexBufferLayoutRef,
        _key: Material2dKey<Self>,
    ) -> Result<(), SpecializedMeshPipelineError> {
        set_blend(descriptor, K::blend());
        add_shared_gpu_layout(descriptor);
        Ok(())
    }
}

/// Shared paint assets and reusable instance slots for kind `K`.
#[derive(Resource)]
struct PaintLane<K: LaneVisual> {
    mesh: Handle<Mesh>,
    buffer: Handle<ShaderBuffer>,
    /// Keeps the look module loaded, so the generated entry shader can import it.
    _look: Handle<Shader>,
    /// One entry per slot handed out so far; freed slots keep stale data.
    instances: Vec<K>,
    free_slots: Vec<u32>,
    is_upload_needed: bool,
}
impl<K: LaneVisual> PaintLane<K> {
    /// Takes a free slot, or the next new one, and writes `instance` into it.
    fn assign_slot(&mut self, instance: K) -> u32 {
        self.is_upload_needed = true;
        if let Some(slot) = self.free_slots.pop() {
            self.instances[slot as usize] = instance;
            return slot;
        }
        self.instances.push(instance);
        (self.instances.len() - 1) as u32
    }

    fn free_slot(&mut self, slot: u32) {
        self.free_slots.push(slot);
    }

    fn write(&mut self, slot: u32, instance: K) {
        self.instances[slot as usize] = instance;
        self.is_upload_needed = true;
    }

    /// Serializes changed instance data as a WGSL `array<Instance>` and clears the upload flag.
    fn take_upload(&mut self) -> Option<Vec<u8>> {
        if !self.is_upload_needed { return None; }
        self.is_upload_needed = false;
        let mut bytes = StorageBuffer::new(Vec::new());
        bytes.write(&self.instances).expect("instances fit in a growable buffer");
        Some(bytes.into_inner())
    }
}

fn entry_shader_source<K: LaneVisual>() -> String {
    format!(
        "import bevy_sprite_render::mesh2d::functions::get_tag;
import bevy_sprite_render::mesh2d::vertex_output::VertexOutput;
import package::{look_module}::{{Instance, look}};

@group(2) @binding(0) var<storage, read> instances: array<Instance>;

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {{
    return look(in, instances[get_tag(in.instance_index)]);
}}
",
        look_module = K::LOOK,
    )
}

fn init_paint_lane<K: LaneVisual>(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    mut shaders: ResMut<Assets<Shader>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut buffers: ResMut<Assets<ShaderBuffer>>,
    mut materials: ResMut<Assets<LaneMaterial<K>>>,
) {
    shaders
        .insert(&entry_shader_id::<K>(PAINT_LANE), Shader::from_wesl(entry_shader_source::<K>(), format!("shaders/lanes/{}.wesl", K::LABEL)))
        .expect("a uuid shader id is always valid");
    // Sized for one instance until the first spawn, since an empty buffer cannot be bound.
    let buffer = buffers.add(ShaderBuffer::with_size(K::min_size().get(), RenderAssetUsages::default()));
    let material = materials.add(ExtendedMaterial2d {
        base: PaintMaterial { instances: buffer.clone(), kind: PhantomData },
        extension: K::Material::default(),
    });
    commands.insert_resource(LaneMaterialHandle::<K>(material));
    commands.insert_resource(PaintLane::<K> {
        mesh: meshes.add(K::mesh()),
        buffer,
        _look: asset_server.load(look_asset_path::<K>()),
        instances: Vec::new(),
        free_slots: Vec::new(),
        is_upload_needed: false,
    });
}

fn on_add_assign_lane_slot<K: LaneVisual>(
    trigger: On<Add<K>>,
    mut commands: Commands,
    mut lane: ResMut<PaintLane<K>>,
    material: Res<LaneMaterialHandle<K>>,
    visuals: Query<&K>,
) {
    let entity = trigger.entity;
    let Ok(visual) = visuals.get(entity) else { return; };
    let slot = lane.assign_slot(visual.clone());
    commands.entity(entity).insert((
        Mesh2d(lane.mesh.clone()),
        MeshMaterial2d(material.0.clone()),
        MeshTag::new(slot),
    ));
}

fn on_remove_free_lane_slot<K: LaneVisual>(
    trigger: On<Remove<K>>,
    mut commands: Commands,
    mut lane: ResMut<PaintLane<K>>,
    tags: Query<&MeshTag>,
) {
    let entity = trigger.entity;
    let Ok(tag) = tags.get(entity) else { return; };
    lane.free_slot(tag.value);
    commands.entity(entity).try_remove::<(Mesh2d, MeshMaterial2d<LaneMaterial<K>>, MeshTag)>();
}

fn upload_paint_instances<K: LaneVisual>(
    mut buffers: ResMut<Assets<ShaderBuffer>>,
    mut lane: ResMut<PaintLane<K>>,
    changed_visuals: Query<(&K, &MeshTag), Changed<K>>,
) {
    for (visual, tag) in changed_visuals.iter() {
        lane.write(tag.value, visual.clone());
    }
    let Some(bytes) = lane.take_upload() else { return; };
    let mut buffer = buffers.get_mut(&lane.buffer).unwrap();
    buffer.clear();
    buffer.extend_from_slice(&bytes);
}

fn mark_bands_reading_beneath<K: LaneVisual>(
    mut bands: ResMut<BandsReadingBeneath>,
    visuals: Query<(&ZDepth, Option<&ViewVisibility>), With<K>>,
) {
    for (layer, visibility) in visuals.iter() {
        if visibility.is_some_and(|visibility| !visibility.get()) { continue; }
        bands.mark(layer.band());
    }
}
