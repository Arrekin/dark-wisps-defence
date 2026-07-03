use bevy::platform::collections::HashMap;
use bevy::prelude::*;

use game_core::prelude::*;

use crate::effects::ModifierContributions;

pub mod prelude {
    pub use super::{
        AttackDamage, AttackRange, AttackSpeed,
        EnergySupplyRange, IncomingDamageMultiplier,
        MaxIntegrityPoints, ModifierBank, ModifierType, MovementSpeed,
    };
}

////////////////////////
////  MODIFIER TYPE ////
////////////////////////

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ModifierType {
    AttackSpeed,
    AttackRange,
    AttackDamage,
    MaxIntegrityPoints,
    MovementSpeed,
    EnergySupplyRange,
    IncomingDamageMultiplier,
}
impl ModifierType {
    fn aggregate(&self, values: impl Iterator<Item = f32>) -> f32 {
        match self {
            Self::AttackSpeed
            | Self::AttackRange
            | Self::AttackDamage
            | Self::MaxIntegrityPoints
            | Self::MovementSpeed
            | Self::EnergySupplyRange => values.fold(0.0, |acc, v| acc + v),
            Self::IncomingDamageMultiplier => values.fold(1.0, f32::max),
        }
    }

    fn materialize(&self, value: f32, entity_commands: &mut EntityCommands) {
        match self {
            Self::AttackSpeed => { entity_commands.insert(AttackSpeed::new(value)); }
            Self::AttackRange => { entity_commands.insert(AttackRange::new(value)); }
            Self::AttackDamage => { entity_commands.insert(AttackDamage::new(value)); }
            Self::MaxIntegrityPoints => { entity_commands.insert(MaxIntegrityPoints::new(value)); }
            Self::MovementSpeed => { entity_commands.insert(MovementSpeed::new(value)); }
            Self::EnergySupplyRange => { entity_commands.insert(EnergySupplyRange::new(value)); }
            Self::IncomingDamageMultiplier => { entity_commands.insert(IncomingDamageMultiplier::new(value)); }
        }
    }
}

#[derive(Component, Clone, Copy, Property)]
#[component(immutable)]
#[require(IntegrityPoints)]
pub struct MaxIntegrityPoints(pub f32);
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
pub struct AttackRange(f32);
#[derive(Component, Default, Clone, Copy, Property)]
#[component(immutable)]
pub struct EnergySupplyRange(f32);
#[derive(Component, Clone, Copy, Property)]
#[component(immutable)]
pub struct IncomingDamageMultiplier(f32);
impl Default for IncomingDamageMultiplier {
    fn default() -> Self { Self::new(1.0) }
}

/////////////////////
// MODIFIER BANK  ///
/////////////////////

/// Per-entity cache of all active modifier contributions, keyed by effect instance entity.
///
/// Updated by effect instance observers, which also re-materialize affected stats immediately.
#[derive(Component, Default)]
pub struct ModifierBank {
    entries: HashMap<ModifierType, HashMap<Entity, f32>>,
}
impl ModifierBank {
    fn insert(&mut self, effect_entity: Entity, stat: ModifierType, value: f32) {
        self.entries.entry(stat).or_default().insert(effect_entity, value);
    }

    fn remove(&mut self, effect_entity: Entity, stat: ModifierType) {
        if let Some(sources) = self.entries.get_mut(&stat) {
            sources.remove(&effect_entity);
        }
    }

    fn aggregate(&self, stat: ModifierType) -> f32 {
        let values = self.entries
            .get(&stat)
            .map(|sources| sources.values().copied())
            .into_iter()
            .flatten();
        stat.aggregate(values)
    }

    /// Applies all contributions from an effect instance to this bank and re-materializes affected stats.
    pub fn apply_contributions(
        &mut self,
        effect_entity: Entity,
        contributions: &ModifierContributions,
        entity_commands: &mut EntityCommands,
    ) {
        for (stat, value) in contributions.0.iter() {
            self.insert(effect_entity, *stat, *value);
            let aggregated = self.aggregate(*stat);
            stat.materialize(aggregated, entity_commands);
        }
    }

    /// Removes all contributions from an effect instance from this bank and re-materializes affected stats.
    pub fn remove_contributions(
        &mut self,
        effect_entity: Entity,
        contributions: &ModifierContributions,
        entity_commands: &mut EntityCommands,
    ) {
        for (stat, _) in contributions.0.iter() {
            self.remove(effect_entity, *stat);
            let aggregated = self.aggregate(*stat);
            stat.materialize(aggregated, entity_commands);
        }
    }
}
