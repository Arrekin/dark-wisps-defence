use bevy::prelude::*;

use alteration::{
    effects::{EffectTarget, ModifierContributions},
    modifiers::{MaxIntegrityPoints, ModifierBank},
};
use game_core::prelude::{IntegrityPoints, Property};

pub(crate) struct ModifiersPlugin;
impl Plugin for ModifiersPlugin {
    fn build(&self, app: &mut App) {
        app
            .add_observer(on_insert_modifier_contributions_apply_to_bank)
            .add_observer(on_remove_modifier_contributions_remove_from_bank)
            .add_observer(on_insert_max_integrity_points_clamp_integrity_points);
    }
}

fn on_insert_modifier_contributions_apply_to_bank(
    trigger: On<Insert<ModifierContributions>>,
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

fn on_remove_modifier_contributions_remove_from_bank(
    trigger: On<Remove<ModifierContributions>>,
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

fn on_insert_max_integrity_points_clamp_integrity_points(
    trigger: On<Insert<MaxIntegrityPoints>>,
    mut integrity_points_components: Query<(&mut IntegrityPoints, &MaxIntegrityPoints)>,
) {
    let Ok((mut integrity_points, max_integrity_points)) = integrity_points_components.get_mut(trigger.entity) else { return; };
    let max = max_integrity_points.get();
    integrity_points.max = max;
    integrity_points.current = integrity_points.current.min(max);
}
