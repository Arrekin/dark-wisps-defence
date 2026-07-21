pub(crate) mod core;
pub(crate) mod goal_clear_quantum_fields;
pub(crate) mod goal_kill_wisps;
pub(crate) mod panel;
pub(crate) mod restriction_time_allowance;

use bevy::prelude::*;
use narrative::prelude::ObjectiveGoalRegistry;
use persistence::prelude::{AppGameLoadSaveExtension, CollectSave};
use states::prelude::MapLoadingStage;

pub struct ObjectivesPlugin;
impl Plugin for ObjectivesPlugin {
    fn build(&self, app: &mut App) {
        app
            .init_resource::<ObjectiveGoalRegistry>()
            .add_observer(core::on_builder_add_spawn_objective)
            .add_observer(core::on_insert_objective_state_sync_markers)
            .add_observer(core::on_objective_activate)
            .add_observer(core::on_goal_state_changed_aggregate)
            // Activation & triggers
            .add_observer(core::on_trigger_fired_activate)
            .add_observer(core::on_objective_satisfied_fire_trigger)
            .add_observer(core::on_remove_activated_by_fail_inactive)
            .add_systems(CollectSave, core::collect_objectives)
            .register_loader(MapLoadingStage::SpawnMapElements, "objectives", core::load_objectives)
            .add_plugins((
                goal_kill_wisps::GoalKillWispsPlugin,
                goal_clear_quantum_fields::GoalClearQuantumFieldsPlugin,
                restriction_time_allowance::RestrictionTimeAllowancePlugin,
                panel::ObjectivesPanelPlugin,
            ))
            ;
    }
}
