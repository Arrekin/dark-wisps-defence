//! Rust-side helpers for materials and the values they pass to their shaders.

use bevy::{
    prelude::*,
    render::render_resource::{BlendComponent, BlendFactor, BlendOperation, BlendState, RenderPipelineDescriptor},
};

/// A stable per-entity phase offset for a shader's animation, so neighboring entities of the same
/// kind do not animate in lockstep.
pub fn seed_from_entity(entity: Entity) -> f32 {
    (entity.index_u32() % 1024) as f32 * 7.31
}

/// Adds color to what is underneath and leaves its alpha alone.
pub const ADDITIVE_BLENDING: BlendState = BlendState {
    color: BlendComponent { src_factor: BlendFactor::One, dst_factor: BlendFactor::One, operation: BlendOperation::Add },
    alpha: BlendComponent { src_factor: BlendFactor::Zero, dst_factor: BlendFactor::One, operation: BlendOperation::Add },
};

/// Sets how every color target of a material's pipeline blends; called from `Material2d::specialize`.
pub fn set_blend(descriptor: &mut RenderPipelineDescriptor, blend: BlendState) {
    for target in descriptor.fragment.iter_mut().flat_map(|fragment| fragment.targets.iter_mut()).flatten() {
        target.blend = Some(blend);
    }
}
