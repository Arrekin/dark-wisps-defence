//! Shared render sink that uploads an entity's [`EffectVisualState`] into a material uniform.
//!
//! Any 2D material that wants to render effect visuals implements [`EffectVisualMaterial`] and
//! embeds an [`EffectVisualUniform`] at binding 5; [`sync_effect_visuals`] then mirrors the
//! aggregated state into it. The system is generic over the material type, so wisps, buildings
//! and units share one implementation — each just registers it for its own material type.

use bevy::render::render_resource::ShaderType;
use bevy::sprite_render::Material2d;

use crate::prelude::*;

/// GPU-side mirror of an entity's active effect visuals, embedded in every effectful material.
///
/// `mask` is a bitfield of active effects (see `BRITTLE_BIT` and friends); `params` is a
/// per-effect parameter bank indexed by slot. The consuming shader reads both.
#[derive(ShaderType, Clone, Debug, Default)]
pub struct EffectVisualUniform {
    pub mask: u32,
    pub params: [Vec4; EFFECT_VISUAL_SLOTS],
}

/// A 2D material that can render aggregated effect visuals.
pub trait EffectVisualMaterial: Material2d {
    fn effects_mut(&mut self) -> &mut EffectVisualUniform;
}

/// Mirrors each changed [`EffectVisualState`] into its material's [`EffectVisualUniform`].
///
/// Registered once per material type; `Changed`-gated so the upload cost scales with effect
/// activity rather than with the number of entities on screen.
pub fn sync_effect_visuals<MaterialT: Asset + EffectVisualMaterial>(
    targets: Query<(&EffectVisualState, &MeshMaterial2d<MaterialT>), Changed<EffectVisualState>>,
    mut materials: ResMut<Assets<MaterialT>>,
) {
    for (state, material_handle) in targets.iter() {
        let Some(material) = materials.get_mut(&material_handle.0) else { continue; };
        let uniform = material.effects_mut();
        uniform.mask = state.mask();
        uniform.params = state.params();
    }
}
