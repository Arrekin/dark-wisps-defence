//! Pipeline layouts for the buffers in `game_core::shared_gpu` and the scene snapshot.
//! Materials opt in during specialization, then import WESL helpers to read shared data.
//! The band, cut and adapted UI passes bind the group at [`SHARED_GPU_GROUP`].
//! Unused group indices below it receive an empty [`gap_layout`].
//!
//! `shaders::common::beneath` samples the scene snapshot captured after a band's displacement.
//! When no snapshot is bound, it reads Bevy's fallback image.

use bevy::render::render_resource::{
    BindGroupLayoutDescriptor, BindGroupLayoutEntry, RenderPipelineDescriptor, SamplerBindingType, ShaderStages, TextureSampleType,
    binding_types::{sampler, storage_buffer_read_only_sized, texture_2d},
};
use strum::IntoEnumIterator;

use game_core::shared_gpu::SharedGpuBinding;

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
