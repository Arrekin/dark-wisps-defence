use bevy::prelude::*;

use alteration::effects::{
    EffectTarget,
    visual::{EffectVisualContribution, EffectVisualState},
};

pub(crate) struct EffectVisualsPlugin;
impl Plugin for EffectVisualsPlugin {
    fn build(&self, app: &mut App) {
        app
            .add_observer(on_insert_effect_visual_contribution_set_state)
            .add_observer(on_remove_effect_visual_contribution_clear_state);
    }
}

fn on_insert_effect_visual_contribution_set_state(
    trigger: On<Insert<EffectVisualContribution>>,
    contributions: Query<(&EffectTarget, &EffectVisualContribution)>,
    mut states: Query<&mut EffectVisualState>,
) {
    let effect_entity = trigger.entity;
    let Ok((effect_target, contribution)) = contributions.get(effect_entity) else { return; };
    let Ok(mut state) = states.get_mut(effect_target.0) else { return; };
    state.set(effect_entity, *contribution);
}

fn on_remove_effect_visual_contribution_clear_state(
    trigger: On<Remove<EffectVisualContribution>>,
    targets: Query<&EffectTarget>,
    mut states: Query<&mut EffectVisualState>,
) {
    let effect_entity = trigger.entity;
    let Ok(effect_target) = targets.get(effect_entity) else { return; };
    let Ok(mut state) = states.get_mut(effect_target.0) else { return; };
    state.clear(effect_entity);
}
