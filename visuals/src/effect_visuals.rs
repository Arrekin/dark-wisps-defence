//! Copies active effect parameters into lane instances so their shaders can render those effects.

use bevy::{ecs::component::Mutable, prelude::*, render::render_resource::ShaderType};

use alteration::effects::prelude::{EFFECT_VISUAL_SLOTS, EffectVisualState};

/// Active effect data for embedding in a lane instance.
/// Field order and types mirror `EffectVisuals` in `assets/shaders/common/effect_visuals.wesl`.
#[derive(ShaderType, Clone, Debug, Default)]
pub struct EffectVisuals {
    /// One bit per active effect type.
    pub mask: u32,
    /// Per-effect parameters, indexed by slot.
    pub params: [Vec4; EFFECT_VISUAL_SLOTS],
}

/// A lane component that can render aggregated effect visuals.
pub trait EffectVisualTarget: Component<Mutability = Mutable> {
    fn effects_mut(&mut self) -> &mut EffectVisuals;
}

/// Copies changed [`EffectVisualState`] values into each entity's [`EffectVisualTarget`].
pub fn sync_effect_visuals<T: EffectVisualTarget>(
    mut targets: Query<(&EffectVisualState, &mut T), Changed<EffectVisualState>>,
) {
    for (state, mut target) in targets.iter_mut() {
        let effects = target.effects_mut();
        effects.mask = state.mask();
        effects.params = state.params();
    }
}
