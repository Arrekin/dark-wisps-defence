//! Stat components, modifier types, and per-entity stat calculation via [`ModifierBank`].
//!
//! ## Future Directions
//!
//! The modifier model is intentionally simple for now. When new features outgrow fixed rules in
//! [`ModifierBank`], consider:
//! - **Per-bank aggregation config** — each [`ModifierBank`] carries its own aggregation rules.
//! - **Self-describing modifiers** — each modifier defines how it behaves, and the bank evaluates
//!   the full modifier family to compute the final stat value.

use bevy::{ecs::query::QueryData, prelude::*};
use smallvec::SmallVec;
use strum::{AsRefStr, EnumString};

use game_core::prelude::*;

use crate::effects::ModifierContributions;

pub mod prelude {
    pub use super::{
        AttackAreaRange, AttackDamage, AttackRangeType, AttackSpeed,
        EnergySupplyRange, IncomingDamageMultiplier,
        MaxAttackRange, MaxIntegrityPoints, MinAttackRange, ModifierBank, ModifierType, MovementSpeed, TurnSpeed,
    };
}

////////////////////////
//  MODIFIER TYPE
////////////////////////

/// Input channel for stat calculation. Effects contribute values keyed by `ModifierType`, which
/// [`ModifierBank`] aggregates into the target's stat components.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, EnumString, AsRefStr)]
pub enum ModifierType {
    AttackSpeed,
    MinAttackRange,
    MaxAttackRange,
    AttackDamage,
    MaxIntegrityPoints,
    MovementSpeedFlat,
    MovementSpeedSlowPercentage,
    EnergySupplyRange,
    IncomingDamageMultiplier,
    TurnSpeed,
}

#[derive(Component, Clone, Copy, Property)]
#[component(immutable)]
#[require(IntegrityPoints)]
pub struct MaxIntegrityPoints(f32);
impl Default for MaxIntegrityPoints {
    fn default() -> Self {
        Self::new(f32::MAX)
    }
}
#[derive(Component, Default, Clone, Copy, Property)]
#[component(immutable)]
pub struct MovementSpeed(f32);
#[derive(Component, Default, Clone, Copy, Property)]
#[component(immutable)]
pub struct AttackSpeed(f32);
#[derive(Component, Default, Clone, Copy, Property)]
#[component(immutable)]
pub struct AttackDamage(f32);
#[derive(Component, Default, Clone, Copy, Property)]
#[component(immutable)]
pub struct MinAttackRange(f32);
#[derive(Component, Default, Clone, Copy, Property)]
#[component(immutable)]
pub struct MaxAttackRange(f32);
impl MaxAttackRange {
    pub fn in_world_units(&self) -> f32 {
        self.get() * CELL_SIZE
    }
}
#[derive(Component, Default, Clone, Copy, Property)]
#[component(immutable)]
pub struct EnergySupplyRange(f32);
#[derive(Component, Clone, Copy, Property)]
#[component(immutable)]
pub struct IncomingDamageMultiplier(f32);
impl Default for IncomingDamageMultiplier {
    fn default() -> Self { Self::new(1.0) }
}
#[derive(Component, Default, Clone, Copy, Property)]
#[component(immutable)]
pub struct TurnSpeed(f32);

////////////////////////
//  ATTACK RANGE
////////////////////////

/// Distance metric for the entity's attack range.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
#[component(immutable)]
pub struct AttackRangeType(pub RangeType);
/// Queries the grid area, distance metric, and limits needed to construct an [`AreaRange`].
#[derive(QueryData)]
pub struct AttackAreaRange {
    area: &'static GridArea,
    range_type: &'static AttackRangeType,
    min_range: &'static MinAttackRange,
    max_range: &'static MaxAttackRange,
}
impl AttackAreaRangeItem<'_, '_> {
    pub fn area_range(&self) -> AreaRange {
        AreaRange { range_type: self.range_type.0, area: *self.area, min: self.min_range.get(), max: self.max_range.get() }
    }
}

////////////////////////
//  MODIFIER BANK
////////////////////////

// Stat limits
/// Floor on materialized [`MovementSpeed`] so slowed entities never come to a full stop.
const MIN_MOVEMENT_SPEED: f32 = 4.0;

/// Per-entity store of active modifier contributions, grouped by [`ModifierType`] and identified by effect entity.
/// Aggregates each group and writes the resulting stat components.
///
/// Removing an effect visits only the groups of the modifiers it contributed. Groups stay once created, so a
/// returning modifier reuses its group.
#[derive(Component, Default)]
pub struct ModifierBank {
    groups: Vec<(ModifierType, SmallVec<[(Entity, f32); 4]>)>,
}
impl ModifierBank {
    fn insert(&mut self, effect_entity: Entity, modifier: ModifierType, value: f32) {
        let index = self.groups.iter().position(|(group_modifier, _)| *group_modifier == modifier).unwrap_or_else(|| {
            self.groups.push((modifier, SmallVec::new()));
            self.groups.len() - 1
        });
        self.groups[index].1.push((effect_entity, value));
    }

    fn remove(&mut self, effect_entity: Entity, modifier: ModifierType) {
        let Some((_, contributions)) = self.groups.iter_mut().find(|(group_modifier, _)| *group_modifier == modifier) else { return; };
        let Some(index) = contributions.iter().position(|(entry_effect, _)| *entry_effect == effect_entity) else { return; };
        contributions.swap_remove(index);
    }

    /// Combines active contributions for `modifier`, defaulting to its identity value when empty.
    fn aggregate(&self, modifier: ModifierType) -> f32 {
        let values = self.groups.iter()
            .find(|(group_modifier, _)| *group_modifier == modifier)
            .into_iter()
            .flat_map(|(_, contributions)| contributions.iter().map(|&(_, value)| value));
        match modifier {
            ModifierType::AttackSpeed
            | ModifierType::MinAttackRange
            | ModifierType::MaxAttackRange
            | ModifierType::AttackDamage
            | ModifierType::MaxIntegrityPoints
            | ModifierType::MovementSpeedFlat
            | ModifierType::EnergySupplyRange
            | ModifierType::TurnSpeed => values.fold(0.0, |acc, v| acc + v),
            ModifierType::MovementSpeedSlowPercentage => values.fold(0.0, f32::max),
            ModifierType::IncomingDamageMultiplier => values.fold(1.0, f32::max),
        }
    }

    fn movement_speed(&self) -> f32 {
        let flat = self.aggregate(ModifierType::MovementSpeedFlat);
        let slow = self.aggregate(ModifierType::MovementSpeedSlowPercentage);
        (flat * (1.0 - slow / 100.0)).max(MIN_MOVEMENT_SPEED)
    }

    /// Recomputes the stat component affected by `modifier` and inserts the updated value.
    fn materialize(&self, modifier: ModifierType, entity_commands: &mut EntityCommands) {
        let value = || self.aggregate(modifier);
        match modifier {
            ModifierType::AttackSpeed => entity_commands.insert(AttackSpeed::new(value())),
            ModifierType::MinAttackRange => entity_commands.insert(MinAttackRange::new(value())),
            ModifierType::MaxAttackRange => entity_commands.insert(MaxAttackRange::new(value())),
            ModifierType::AttackDamage => entity_commands.insert(AttackDamage::new(value())),
            ModifierType::MaxIntegrityPoints => entity_commands.insert(MaxIntegrityPoints::new(value())),
            ModifierType::MovementSpeedFlat | ModifierType::MovementSpeedSlowPercentage => {
                entity_commands.insert(MovementSpeed::new(self.movement_speed()))
            }
            ModifierType::EnergySupplyRange => entity_commands.insert(EnergySupplyRange::new(value())),
            ModifierType::IncomingDamageMultiplier => entity_commands.insert(IncomingDamageMultiplier::new(value())),
            ModifierType::TurnSpeed => entity_commands.insert(TurnSpeed::new(value())),
        };
    }

    /// Applies all contributions from an effect instance to this bank and re-materializes affected stats.
    pub fn apply_contributions(
        &mut self,
        effect_entity: Entity,
        contributions: &ModifierContributions,
        entity_commands: &mut EntityCommands,
    ) {
        for (modifier, value) in contributions.iter() {
            self.insert(effect_entity, modifier, value);
            self.materialize(modifier, entity_commands);
        }
    }

    /// Removes all contributions from an effect instance from this bank and re-materializes affected stats.
    pub fn remove_contributions(
        &mut self,
        effect_entity: Entity,
        contributions: &ModifierContributions,
        entity_commands: &mut EntityCommands,
    ) {
        for (modifier, _) in contributions.iter() {
            self.remove(effect_entity, modifier);
            self.materialize(modifier, entity_commands);
        }
    }
}
