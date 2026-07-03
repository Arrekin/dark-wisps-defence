use bevy::platform::collections::HashMap;
use bevy::prelude::*;

// Effect identity shared by every render sink. Bits are OR-ed into the target's effect mask;
// slots index its parameter bank. Each value has a matching constant in the consuming shaders.
pub const EFFECT_VISUAL_SLOTS: usize = 8;

/// Visual footprint an effect instance contributes to its target.
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
/// derived mask and parameter bank are read by the entity's render sink.
#[derive(Component, Default)]
pub struct EffectVisualState {
    contributions: HashMap<Entity, EffectVisualContribution>,
    mask: u32,
    params: [Vec4; EFFECT_VISUAL_SLOTS],
}
impl EffectVisualState {
    pub fn mask(&self) -> u32 {
        self.mask
    }

    pub fn params(&self) -> [Vec4; EFFECT_VISUAL_SLOTS] {
        self.params
    }

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
        self.mask = 0;
        self.params = [Vec4::ZERO; EFFECT_VISUAL_SLOTS];
        for contribution in self.contributions.values() {
            self.mask |= contribution.bit;
            if contribution.slot < EFFECT_VISUAL_SLOTS {
                self.params[contribution.slot] = contribution.params;
            }
        }
    }
}
