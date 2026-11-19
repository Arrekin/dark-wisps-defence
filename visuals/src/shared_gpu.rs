//! # Shared GPU data
//!
//! Shared world data and static shader resources bound at [`SHARED_GPU_GROUP`] across the band,
//! cut, and adapted UI passes. [`SharedGpuBinding`] enumerates every binding in the group. Each
//! buffer binding owns a [`ShaderBuffer`] updated by its domain through [`SharedGpuWriter`];
//! rendering combines the buffers, textures, and samplers into a single bind group so materials
//! can read them without storing individual handles.
//!
//! Materials opt in during specialization (via [`specialize_with_shared_gpu!`] or
//! [`specialize_ui_with_shared_gpu!`]) and import WESL modules to read the bindings.
//! Unused group indices below [`SHARED_GPU_GROUP`] receive an empty [`gap_layout`].

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
            binding_types::{sampler, storage_buffer_read_only_sized, texture_2d, texture_3d},
        },
        storage::ShaderBuffer,
    },
};
use bytemuck::NoUninit;
use strum::{EnumIter, IntoEnumIterator};

// ── Bindings ───────────────────────────────────────────────────────────────────────────

/// Every binding in the shared GPU group, in binding order. Each discriminant must match its WESL
/// binding number; a new binding takes the next free number.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, EnumIter)]
#[repr(u32)]
pub enum SharedGpuBinding {
    /// `GpuMapInfo`, read through `shaders::common::map_info`.
    MapInfo = 100,
    /// The current game clock instant, one `ClockInstant`, read through `shaders::common::clock`.
    GameClock = 101,
    /// Dark ore fill, one `f32` per cell, read through `shaders::dark_ore::fill`.
    DarkOreFill = 102,
    /// Obstacle cells, one packed `u32` per cell, read through `shaders::grids::obstacles`.
    ObstacleCells = 103,
    /// Wall style index and shape code per cell, read through `shaders::walls::styles`.
    WallCells = 104,
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
    /// The band's scene snapshot after displacement, read through `shaders::common::beneath`.
    /// Bevy's fallback image stands in where no snapshot is bound.
    BeneathTexture = 110,
    /// The snapshot's linear sampler, read through `shaders::common::beneath`.
    BeneathSampler = 111,
    /// Start offsets into `FloorCoverCellEntries`, one `u32` per cell plus a final end offset.
    /// Read through `shaders::grids::floor_cover`.
    FloorCoverCellOffsets = 112,
    /// Floor cover entries concatenated in cell order, read through `shaders::grids::floor_cover`.
    FloorCoverCellEntries = 113,
    /// The epoch of Bevy's shader clock, one `u32`, read through `shaders::common::clock`.
    ShaderClockEpoch = 114,
    /// Tileable 3D value-noise lattice texture, read through `shaders::common::noise`.
    NoiseValue3d = 115,
    /// Repeating linear sampler for noise textures, read through `shaders::common::noise`.
    NoiseSampler = 116,
    /// Tileable 2D gradient-noise lattice texture, read through `shaders::common::noise`.
    NoiseGradient2d = 117,
    /// Tileable 2D value-noise lattice texture, read through `shaders::common::noise`.
    NoiseValue2d = 118,
}
impl SharedGpuBinding {
    pub const fn index(self) -> u32 {
        self as u32
    }

    pub const fn kind(self) -> SharedGpuBindingKind {
        match self {
            Self::BeneathTexture | Self::NoiseGradient2d | Self::NoiseValue2d => SharedGpuBindingKind::Texture2d,
            Self::NoiseValue3d => SharedGpuBindingKind::Texture3d,
            Self::BeneathSampler | Self::NoiseSampler => SharedGpuBindingKind::Sampler,
            Self::MapInfo
            | Self::GameClock
            | Self::DarkOreFill
            | Self::ObstacleCells
            | Self::WallCells
            | Self::WallStyles
            | Self::EmissionsEnergy
            | Self::FieldShapes
            | Self::FieldCellOffsets
            | Self::FieldCellEntries
            | Self::FloorCoverCellOffsets
            | Self::FloorCoverCellEntries
            | Self::ShaderClockEpoch => SharedGpuBindingKind::Buffer,
        }
    }

    /// Iterates over all storage-buffer bindings.
    pub fn buffers() -> impl Iterator<Item = Self> {
        Self::iter().filter(|binding| binding.kind() == SharedGpuBindingKind::Buffer)
    }
}

/// Resource type of a [`SharedGpuBinding`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SharedGpuBindingKind {
    /// A read-only storage buffer.
    Buffer,
    /// A filterable float 2D texture.
    Texture2d,
    /// A filterable float 3D texture.
    Texture3d,
    /// A filtering sampler.
    Sampler,
}

// ── Buffers ────────────────────────────────────────────────────────────────────────────

/// Zeroed fallback for empty input; GPU bindings require a nonempty buffer.
const PLACEHOLDER: [u32; 1] = [0];

/// Persistent [`ShaderBuffer`] handles for all storage-buffer [`SharedGpuBinding`]s.
/// Each buffer starts with a zeroed placeholder until its owner uploads data.
#[derive(Resource, ExtractResource, Clone)]
#[extract_app(RenderApp)]
pub struct SharedGpuBuffers {
    handles: HashMap<SharedGpuBinding, Handle<ShaderBuffer>>,
}
impl FromWorld for SharedGpuBuffers {
    fn from_world(world: &mut World) -> Self {
        let mut buffers = world.resource_mut::<Assets<ShaderBuffer>>();
        let handles = SharedGpuBinding::buffers()
            .map(|binding| (binding, buffers.add(ShaderBuffer::new(PLACEHOLDER.to_vec(), RenderAssetUsages::default()))))
            .collect();
        Self { handles }
    }
}
impl SharedGpuBuffers {
    /// Panics if `binding` is not a [`SharedGpuBindingKind::Buffer`].
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
    /// Panics if `binding` is not a [`SharedGpuBindingKind::Buffer`].
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

/// Layout containing every [`SharedGpuBinding`].
pub fn shared_gpu_layout() -> BindGroupLayoutDescriptor {
    let entries: Vec<BindGroupLayoutEntry> = SharedGpuBinding::iter()
        .map(|binding| {
            let entry = match binding.kind() {
                SharedGpuBindingKind::Buffer => storage_buffer_read_only_sized(false, None),
                SharedGpuBindingKind::Texture2d => texture_2d(TextureSampleType::Float { filterable: true }),
                SharedGpuBindingKind::Texture3d => texture_3d(TextureSampleType::Float { filterable: true }),
                SharedGpuBindingKind::Sampler => sampler(SamplerBindingType::Filtering),
            };
            entry.build(binding.index(), ShaderStages::VERTEX_FRAGMENT)
        })
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
