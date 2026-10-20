//! # Shared GPU data
//!
//! Shared world data for shaders, with one `ShaderBuffer` per [`SharedGpuBinding`].
//! Each domain uploads its data through [`SharedGpuWriter`]. Rendering combines these buffers
//! into one bind group, so materials can read them without storing individual buffer handles.
//!
//! Materials opt in during specialization, then import WESL helpers to read shared data.
//! The band, cut and adapted UI passes bind the group at [`SHARED_GPU_GROUP`].
//! Unused group indices below it receive an empty [`gap_layout`].
//!
//! `shaders::common::beneath` samples the scene snapshot captured after a band's displacement.
//! When no snapshot is bound, it reads Bevy's fallback image.

use bevy::{
    asset::RenderAssetUsages,
    ecs::system::SystemParam,
    platform::collections::HashMap,
    prelude::*,
    render::{
        RenderApp,
        extract_resource::ExtractResource,
        render_resource::{
            BindGroupLayoutDescriptor, BindGroupLayoutEntry, RenderPipelineDescriptor, SamplerBindingType, ShaderStages, TextureSampleType,
            binding_types::{sampler, storage_buffer_read_only_sized, texture_2d},
        },
        storage::ShaderBuffer,
    },
};
use bytemuck::NoUninit;
use strum::{EnumIter, IntoEnumIterator};

// ── Buffers ────────────────────────────────────────────────────────────────────────────

/// Buffer bindings in the shared GPU group. Each discriminant must match its WESL binding number.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, EnumIter)]
#[repr(u32)]
pub enum SharedGpuBinding {
    /// `GpuMapInfo`, read through `shaders::common::map_info`.
    MapInfo = 100,
    /// The wrapped game time, one `f32`, read through `shaders::common::game_time`.
    GameTime = 101,
    /// Dark ore fill, one `f32` per cell, read through `shaders::dark_ore::fill`.
    DarkOreFill = 102,
    /// Obstacle cells, one packed `u32` per cell, read through `shaders::grids::obstacles`.
    ObstacleCells = 103,
    /// Wall style index per cell, read through `shaders::walls::styles`.
    WallStyleCells = 104,
    /// The wall style table, read through `shaders::walls::styles`.
    WallStyles = 105,
    /// Emitted energy, one `f32` per cell, read through `shaders::grids::emissions`.
    EmissionsEnergy = 106,
    /// The field table, one `FieldShape` per slot, read through `shaders::fields::fields`.
    FieldShapes = 107,
    /// Start offsets into `FieldCellEntries`, one `u32` per cell plus a final end offset.
    /// Read through `shaders::fields::fields`.
    FieldCellOffsets = 108,
    /// Candidate field slots concatenated in cell order, read through `shaders::fields::fields`.
    FieldCellEntries = 109,
}
impl SharedGpuBinding {
    pub fn index(self) -> u32 {
        self as u32
    }
}

/// Zeroed fallback for empty input; GPU bindings require a nonempty buffer.
const PLACEHOLDER: [u32; 1] = [0];

/// Persistent buffer handles, one per [`SharedGpuBinding`].
/// Each buffer starts with a zeroed placeholder until its owner uploads data.
#[derive(Resource, ExtractResource, Clone)]
#[extract_app(RenderApp)]
pub struct SharedGpuBuffers {
    handles: HashMap<SharedGpuBinding, Handle<ShaderBuffer>>,
}
impl FromWorld for SharedGpuBuffers {
    fn from_world(world: &mut World) -> Self {
        let mut buffers = world.resource_mut::<Assets<ShaderBuffer>>();
        let handles = SharedGpuBinding::iter()
            .map(|binding| (binding, buffers.add(ShaderBuffer::new(PLACEHOLDER.to_vec(), RenderAssetUsages::default()))))
            .collect();
        Self { handles }
    }
}
impl SharedGpuBuffers {
    pub fn get(&self, binding: SharedGpuBinding) -> &Handle<ShaderBuffer> {
        &self.handles[&binding]
    }
}

/// Writes the contents of shared GPU buffers.
#[derive(SystemParam)]
pub struct SharedGpuWriter<'w> {
    buffers: ResMut<'w, Assets<ShaderBuffer>>,
    shared_gpu_buffers: Res<'w, SharedGpuBuffers>,
}
impl SharedGpuWriter<'_> {
    /// Replaces a binding's buffer contents, using a zeroed placeholder for empty input.
    /// Values are copied as raw bytes; their layout must match the shader's storage-buffer layout.
    pub fn write<T: NoUninit>(&mut self, binding: SharedGpuBinding, values: impl IntoIterator<Item = T>) {
        let mut buffer = self.buffers.get_mut(self.shared_gpu_buffers.get(binding)).unwrap();
        Self::fill(&mut buffer, values);
    }

    /// Like [`Self::write`], but retains capacity. When more space is needed, grows to the next
    /// power of two in bytes. Never shrinks; bytes beyond the written data are unspecified.
    /// Keeping the buffer size stable avoids GPU reallocation and the resulting bind group rebuild.
    pub fn write_with_capacity<T: NoUninit>(&mut self, binding: SharedGpuBinding, values: impl IntoIterator<Item = T>) {
        let mut buffer = self.buffers.get_mut(self.shared_gpu_buffers.get(binding)).unwrap();
        let capacity = buffer.buffer_size();
        Self::fill(&mut buffer, values);
        let size = buffer.buffer_size();
        buffer.resize_buffer(if size <= capacity { capacity } else { size.next_power_of_two() });
    }

    fn fill<T: NoUninit>(buffer: &mut ShaderBuffer, values: impl IntoIterator<Item = T>) {
        let mut values = values.into_iter().peekable();
        buffer.clear();
        if values.peek().is_some() {
            buffer.extend(values);
        } else {
            buffer.extend(PLACEHOLDER);
        }
    }
}

// ── Layout ─────────────────────────────────────────────────────────────────────────────

/// The bind group index of the shared GPU data.
pub const SHARED_GPU_GROUP: usize = 3;

// Beneath snapshot bindings, mirrored in `assets/shaders/common/beneath.wesl`.
pub const BENEATH_TEXTURE_BINDING: u32 = 110;
pub const BENEATH_SAMPLER_BINDING: u32 = 111;

/// Layout containing every [`SharedGpuBinding`], plus the scene snapshot texture and sampler.
pub fn shared_gpu_layout() -> BindGroupLayoutDescriptor {
    let entries: Vec<BindGroupLayoutEntry> = SharedGpuBinding::iter()
        .map(|binding| storage_buffer_read_only_sized(false, None).build(binding.index(), ShaderStages::VERTEX_FRAGMENT))
        .chain([
            texture_2d(TextureSampleType::Float { filterable: true }).build(BENEATH_TEXTURE_BINDING, ShaderStages::VERTEX_FRAGMENT),
            sampler(SamplerBindingType::Filtering).build(BENEATH_SAMPLER_BINDING, ShaderStages::VERTEX_FRAGMENT),
        ])
        .collect();
    BindGroupLayoutDescriptor::new("shared_gpu_layout", &entries)
}

/// An empty group, filling a pipeline's unused groups below [`SHARED_GPU_GROUP`].
pub fn gap_layout() -> BindGroupLayoutDescriptor {
    BindGroupLayoutDescriptor::new("shared_gpu_gap_layout", &[])
}

/// Adds the shared GPU layout, filling unused lower group indices with empty layouts.
/// Call from `Material2d::specialize` or `UiMaterial::specialize`.
pub fn add_shared_gpu_layout(descriptor: &mut RenderPipelineDescriptor) {
    let layout = &mut descriptor.layout;
    assert!(layout.len() <= SHARED_GPU_GROUP, "the pipeline already uses the shared GPU group's index");
    layout.resize_with(SHARED_GPU_GROUP, gap_layout);
    layout.push(shared_gpu_layout());
}

/// Implements specialization for a `Material2d` that reads shared GPU data.
/// Invoke inside `impl Material2d`. If specialization is already implemented, call
/// [`add_shared_gpu_layout`] from that method.
#[macro_export]
macro_rules! specialize_with_shared_gpu {
    () => {
        fn specialize(
            _pipeline: &::bevy::sprite_render::Material2dPipeline,
            descriptor: &mut ::bevy::render::render_resource::RenderPipelineDescriptor,
            _layout: &::bevy::mesh::MeshVertexBufferLayoutRef,
            _key: ::bevy::sprite_render::Material2dKey<Self>,
        ) -> ::core::result::Result<(), ::bevy::render::render_resource::SpecializedMeshPipelineError> {
            $crate::shared_gpu::add_shared_gpu_layout(descriptor);
            Ok(())
        }
    };
}

/// Implements specialization for a `UiMaterial` that reads shared GPU data.
/// Invoke inside `impl UiMaterial`. If specialization is already implemented, call
/// [`add_shared_gpu_layout`] from that method.
#[macro_export]
macro_rules! specialize_ui_with_shared_gpu {
    () => {
        fn specialize(
            descriptor: &mut ::bevy::render::render_resource::RenderPipelineDescriptor,
            _key: ::bevy::ui_render::ui_material::UiMaterialKey<Self>,
        ) {
            $crate::shared_gpu::add_shared_gpu_layout(descriptor);
        }
    };
}
