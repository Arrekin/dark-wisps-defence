use bevy::prelude::*;
use strum::{AsRefStr, EnumIter};

/// Registry of available goal types — feeds the editor's "Add" menu.
/// Populated by each goal module's plugin via `register_objective_goal`.
#[derive(Resource, Default, Clone)]
pub struct ObjectiveGoalRegistry {
    pub entries: Vec<ObjectiveGoalEntry>,
}

/// Grouping for the editor's "Add Goal" menu. Adding a new group = adding a
/// variant here (compile-time, no typos).
#[derive(Clone, Copy, PartialEq, Eq, Hash, EnumIter, AsRefStr)]
pub enum ObjectiveGoalGroup {
    Goals,
    Restrictions,
}

/// One entry per goal type. `spawn` creates a goal entity with the config
/// component + `ObjectiveGoalOf(objective)` on the given objective entity.
#[derive(Clone, Copy)]
pub struct ObjectiveGoalEntry {
    pub name: &'static str,
    pub group: ObjectiveGoalGroup,
    pub spawn: fn(&mut Commands, Entity),
}

/// App extension for registering goal types.
pub trait AppObjectiveGoalExt {
    fn register_objective_goal(
        &mut self,
        name: &'static str,
        group: ObjectiveGoalGroup,
        spawn: fn(&mut Commands, Entity),
    ) -> &mut Self;
}

impl AppObjectiveGoalExt for App {
    fn register_objective_goal(
        &mut self,
        name: &'static str,
        group: ObjectiveGoalGroup,
        spawn: fn(&mut Commands, Entity),
    ) -> &mut Self {
        self.world_mut()
            .resource_mut::<ObjectiveGoalRegistry>()
            .entries
            .push(ObjectiveGoalEntry { name, group, spawn });
        self
    }
}
