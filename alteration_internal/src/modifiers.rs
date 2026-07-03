use bevy::prelude::*;

use alteration::effects::{EffectTarget, ModifierContributions};
use alteration::modifiers::{MaxIntegrityPoints, ModifierBank};
use game_core::prelude::{IntegrityPoints, Property};

pub struct ModifiersPlugin;
impl Plugin for ModifiersPlugin {
    fn build(&self, app: &mut App) {
        app
            .add_observer(apply_modifier_contributions_on_insert)
            .add_observer(remove_modifier_contributions_on_remove)
            .add_observer(clamp_integrity_points_to_max_on_insert)
            ;
    }
}

fn apply_modifier_contributions_on_insert(
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
    bank.apply_contributions(effect_entity, contributions, &mut entity_commands);
}

fn remove_modifier_contributions_on_remove(
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
    bank.remove_contributions(effect_entity, contributions, &mut entity_commands);
}

fn clamp_integrity_points_to_max_on_insert(
    trigger: On<Insert, MaxIntegrityPoints>,
    mut integrity_points_components: Query<(&mut IntegrityPoints, &MaxIntegrityPoints)>,
) {
    let Ok((mut integrity_points, max_integrity_points)) = integrity_points_components.get_mut(trigger.entity) else { return; };
    integrity_points.max = max_integrity_points.get();
    if integrity_points.current > max_integrity_points.get() {
        integrity_points.current = max_integrity_points.get();
    }
}
