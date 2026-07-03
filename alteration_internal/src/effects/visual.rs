use bevy::prelude::*;

use alteration::effects::EffectTarget;
use alteration::effects::visual::{EffectVisualContribution, EffectVisualState};

pub struct EffectVisualsPlugin;
impl Plugin for EffectVisualsPlugin {
    fn build(&self, app: &mut App) {
        app
            .add_observer(apply_effect_visual_contribution_on_insert)
            .add_observer(clear_effect_visual_contribution_on_remove)
            ;
    }
}

fn apply_effect_visual_contribution_on_insert(
    trigger: On<Insert, EffectVisualContribution>,
    contributions: Query<(&EffectTarget, &EffectVisualContribution)>,
    mut states: Query<&mut EffectVisualState>,
) {
    let effect_entity = trigger.entity;
    let Ok((effect_target, contribution)) = contributions.get(effect_entity) else { return; };
    let Ok(mut state) = states.get_mut(effect_target.0) else { return; };
    state.set(effect_entity, *contribution);
}

fn clear_effect_visual_contribution_on_remove(
    trigger: On<Remove, EffectVisualContribution>,
    targets: Query<&EffectTarget>,
    mut states: Query<&mut EffectVisualState>,
) {
    let effect_entity = trigger.entity;
    let Ok(effect_target) = targets.get(effect_entity) else { return; };
    let Ok(mut state) = states.get_mut(effect_target.0) else { return; };
    state.clear(effect_entity);
}
