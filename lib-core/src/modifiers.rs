use strum::{AsRefStr, EnumString};

use crate::lib_prelude::*;

pub mod modifiers_prelude {
    pub use super::{
        ModifierType, ModifierBank,
        MaxHealth, MovementSpeed, AttackSpeed, AttackDamage, AttackRange,
        EnergySupplyRange, IncomingDamageMultiplier,
    };
}

pub struct ModifiersPlugin;
impl Plugin for ModifiersPlugin {
    fn build(&self, app: &mut App) {
        app
            .add_observer(ModifierBank::on_modifier_contributions_added)
            .add_observer(ModifierBank::on_modifier_contributions_removed)
            .add_observer(MaxHealth::on_insert)
            ;
    }
}

////////////////////////
////  MODIFIER TYPE ////
////////////////////////

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, AsRefStr, EnumString)]
pub enum ModifierType {
    AttackSpeed,
    AttackRange,
    AttackDamage,
    MaxHealth,
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
            | Self::MaxHealth
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
            Self::MaxHealth => { entity_commands.insert(MaxHealth::new(value)); }
            Self::MovementSpeed => { entity_commands.insert(MovementSpeed::new(value)); }
            Self::EnergySupplyRange => { entity_commands.insert(EnergySupplyRange::new(value)); }
            Self::IncomingDamageMultiplier => { entity_commands.insert(IncomingDamageMultiplier::new(value)); }
        }
    }
}

#[derive(Component, Clone, Copy, Property)]
#[component(immutable)]
#[require(Health)]
pub struct MaxHealth(pub f32);
impl MaxHealth {   
    fn on_insert(
        trigger: On<Insert, MaxHealth>,
        mut healths: Query<(&mut Health, &MaxHealth)>,
    ) {
        let Ok((mut health, max_health)) = healths.get_mut(trigger.entity) else { return; };
        health.max = max_health.get();
        if health.current > max_health.get() {
            health.current = max_health.get();
        }
    }
}
impl Default for MaxHealth {
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

    fn on_modifier_contributions_added(
        trigger: On<Insert, ModifierContributions>,
        mut commands: Commands,
        instances: Query<(&EffectTarget, &ModifierContributions)>,
        mut banks: Query<&mut ModifierBank>,
    ) {
        let effect_entity = trigger.entity;
        let Ok((effect_target, contributions)) = instances.get(effect_entity) else { return; };
        let target_entity = effect_target.0;
        let Ok(mut bank) = banks.get_mut(target_entity) else { return; };
        let mut entity_commands = commands.entity(target_entity);
        for (stat, value) in contributions.0.iter() {
            bank.insert(effect_entity, *stat, *value);
            let aggregated = bank.aggregate(*stat);
            stat.materialize(aggregated, &mut entity_commands);
        }
    }

    fn on_modifier_contributions_removed(
        trigger: On<Remove, ModifierContributions>,
        mut commands: Commands,
        instances: Query<(&EffectTarget, &ModifierContributions)>,
        mut banks: Query<&mut ModifierBank>,
    ) {
        let effect_entity = trigger.entity;
        let Ok((effect_target, contributions)) = instances.get(effect_entity) else { return; };
        let target_entity = effect_target.0;
        let Ok(mut bank) = banks.get_mut(target_entity) else { return; };
        let mut entity_commands = commands.entity(target_entity);
        for (stat, _) in contributions.0.iter() {
            bank.remove(effect_entity, *stat);
            let aggregated = bank.aggregate(*stat);
            stat.materialize(aggregated, &mut entity_commands);
        }
    }
}
