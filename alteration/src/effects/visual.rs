use bevy::{ecs::component::Mutable, platform::collections::HashMap, prelude::*, render::render_resource::ShaderType};

// Effect identity shared by every render sink. Bits are OR-ed into the target's effect mask;
// slots index its parameter bank. Each value has a matching constant in the consuming shaders.
pub const EFFECT_VISUAL_SLOTS: usize = 8;

// ── Effect bit/slot assignments ─────────────────────────────────────────────
// One place to see which bits and slots are taken. Add new effects here.
pub const BRITTLE_BIT: u32 = 1 << 0;
pub const BRITTLE_SLOT: usize = 0;

/// Visual parameters contributed by an active effect instance to its target.
///
/// Inserted alongside the effect's gameplay components, often via `#[require]` on the effect
/// marker. `bit` is OR-ed into the target's effect mask; `params` are written into the parameter
/// bank at `slot` for the sink to interpret.
#[derive(Component, Clone, Copy)]
pub struct EffectVisualContribution {
    pub bit: u32,
    pub slot: usize,
    pub params: Vec4,
}
impl EffectVisualContribution {
    pub const fn new(bit: u32, slot: usize, params: Vec4) -> Self {
        Self { bit, slot, params }
    }
}

/// Per-target aggregate of every active [`EffectVisualContribution`] pointing at it.
///
/// Contributions are keyed by effect instance entity so they stack and unstack cleanly. The
/// derived [`EffectVisuals`] are read by the entity's render sink.
#[derive(Component, Default)]
pub struct EffectVisualState {
    contributions: HashMap<Entity, EffectVisualContribution>,
    /// Aggregated mask and parameters; updated through [`Self::set`] and [`Self::clear`].
    pub visuals: EffectVisuals,
}
impl EffectVisualState {
    pub fn set(&mut self, effect_entity: Entity, contribution: EffectVisualContribution) {
        self.contributions.insert(effect_entity, contribution);
        self.recompute();
    }

    pub fn clear(&mut self, effect_entity: Entity) {
        self.contributions.remove(&effect_entity);
        self.recompute();
    }

    fn recompute(&mut self) {
        // Bits OR together; a slot is overwritten by whichever contribution holds it. If an
        // effect ever both stacks and carries per-instance params in one slot, that slot needs
        // an explicit combine rule (max/sum/latest), like `ModifierType::aggregate`.
        self.visuals = EffectVisuals::default();
        for contribution in self.contributions.values() {
            self.visuals.mask |= contribution.bit;
            if contribution.slot < EFFECT_VISUAL_SLOTS {
                self.visuals.params[contribution.slot] = contribution.params;
            }
        }
    }
}

/// Active effect data for embedding in a lane instance.
/// Field order and types mirror `EffectVisuals` in `assets/shaders/common/effect_visuals.wesl`.
#[derive(ShaderType, Clone, Copy, Debug, Default)]
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
/// The render sink's domain registers it once per target type.
pub fn sync_effect_visuals<T: EffectVisualTarget>(
    mut targets: Query<(&EffectVisualState, &mut T), Changed<EffectVisualState>>,
) {
    for (state, mut target) in targets.iter_mut() {
        *target.effects_mut() = state.visuals;
    }
}
