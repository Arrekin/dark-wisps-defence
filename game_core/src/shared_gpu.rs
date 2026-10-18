//! # Shared GPU data
//!
//! Shared world data for shaders, with one `ShaderBuffer` per [`SharedGpuBinding`].
//! Each domain uploads its data through [`SharedGpuWriter`]. Rendering combines these buffers
//! into one bind group, so materials can read them without storing individual buffer handles.

use bevy::{
    asset::RenderAssetUsages,
    ecs::system::SystemParam,
    platform::collections::HashMap,
    prelude::*,
    render::{RenderApp, extract_resource::ExtractResource, storage::ShaderBuffer},
};
use bytemuck::NoUninit;
use strum::{EnumIter, IntoEnumIterator};

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
}
impl SharedGpuBinding {
    pub fn index(self) -> u32 {
        self as u32
    }
}

/// Contents of a shared buffer that has no data, since an empty buffer cannot be bound.
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
        let mut values = values.into_iter().peekable();
        let mut buffer = self.buffers.get_mut(self.shared_gpu_buffers.get(binding)).unwrap();
        buffer.clear();
        if values.peek().is_some() {
            buffer.extend(values);
        } else {
            buffer.extend(PLACEHOLDER);
        }
    }
}
