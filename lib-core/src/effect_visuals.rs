//! # Effect Visuals
//!
//! Bridges gameplay effect instances to whatever renders their visual on the target entity.
//!
//! An effect instance that wants a visual carries an [`EffectVisualContribution`] describing the
//! bit it sets and the parameters it feeds. A per-target [`EffectVisualState`] aggregates every
//! contribution pointing at that entity into a single bitmask and parameter bank, which a render
//! sink (a material uniform, an icon row, ...) reads.
//!
//! This mirrors the [`crate::modifiers::ModifierBank`] flow: decentralized contributions on
//! effect instances, aggregated per target, materialized into a downstream sink. The components
//! are entity-agnostic — wisps, buildings and units all aggregate the same way; only the sink
//! that consumes [`EffectVisualState`] differs per entity type.

use crate::lib_prelude::*;

pub mod effect_visuals_prelude {
    pub use super::{
        EffectVisualContribution, EffectVisualState, EFFECT_VISUAL_SLOTS,
        BRITTLE_BIT, BRITTLE_SLOT,
    };
}

pub struct EffectVisualsPlugin;
impl Plugin for EffectVisualsPlugin {
    fn build(&self, app: &mut App) {
        app
            .add_observer(EffectVisualState::on_contribution_added)
            .add_observer(EffectVisualState::on_contribution_removed)
            ;
    }
}

// Effect identity shared by every render sink. Bits are OR-ed into the target's effect mask;
// slots index its parameter bank. Each value has a matching constant in the consuming shaders.
pub const EFFECT_VISUAL_SLOTS: usize = 8;
pub const BRITTLE_BIT: u32 = 1 << 0;
pub const BRITTLE_SLOT: usize = 0;

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

    fn set(&mut self, effect_entity: Entity, contribution: EffectVisualContribution) {
        self.contributions.insert(effect_entity, contribution);
        self.recompute();
    }

    fn clear(&mut self, effect_entity: Entity) {
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

    fn on_contribution_added(
        trigger: On<Insert, EffectVisualContribution>,
        contributions: Query<(&EffectTarget, &EffectVisualContribution)>,
        mut states: Query<&mut EffectVisualState>,
    ) {
        let effect_entity = trigger.entity;
        let Ok((effect_target, contribution)) = contributions.get(effect_entity) else { return; };
        let Ok(mut state) = states.get_mut(effect_target.0) else { return; };
        state.set(effect_entity, *contribution);
    }

    fn on_contribution_removed(
        trigger: On<Remove, EffectVisualContribution>,
        targets: Query<&EffectTarget>,
        mut states: Query<&mut EffectVisualState>,
    ) {
        let effect_entity = trigger.entity;
        let Ok(effect_target) = targets.get(effect_entity) else { return; };
        let Ok(mut state) = states.get_mut(effect_target.0) else { return; };
        state.clear(effect_entity);
    }
}
