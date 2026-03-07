use strum::{AsRefStr, EnumString};

use crate::lib_prelude::*;

pub mod modifiers_prelude {
    pub use super::{
        ModifierType, ModifierBank,
        MaxIntegrityPoints, MovementSpeed, AttackSpeed, AttackDamage, AttackRange,
        EnergySupplyRange, IncomingDamageMultiplier,
    };
}

pub struct ModifiersPlugin;
impl Plugin for ModifiersPlugin {
    fn build(&self, app: &mut App) {
        app
            .add_observer(ModifierBank::on_modifier_contributions_added)
            .add_observer(ModifierBank::on_modifier_contributions_removed)
            .add_observer(MaxIntegrityPoints::on_insert)
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
impl MaxIntegrityPoints {   
    fn on_insert(
        trigger: On<Insert, MaxIntegrityPoints>,
        mut integrity_points_components: Query<(&mut IntegrityPoints, &MaxIntegrityPoints)>,
    ) {
        let Ok((mut integrity_points, max_integrity_points)) = integrity_points_components.get_mut(trigger.entity) else { return; };
        integrity_points.max = max_integrity_points.get();
        if integrity_points.current > max_integrity_points.get() {
            integrity_points.current = max_integrity_points.get();
        }
    }
}
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
