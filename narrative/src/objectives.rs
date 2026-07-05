use strum::{AsRefStr, EnumString};

use bevy::prelude::*;

use game_core::prelude::SSS;

#[derive(Copy, Clone, Debug)]
pub enum ObjectiveType {
    ClearAllQuantumFields,
    // TODO: The `usize` target is redundant with `ObjectiveKillWisps.target_amount`.
    //       It's only consumed on fresh spawn; save/load ignore it. Consider moving
    //       the target out of the enum and into the objective definition flow.
    KillWisps(usize),
}

#[derive(Component, Clone, Debug)]
pub struct ObjectiveDetails {
    pub id_name: String,
    pub objective_type: ObjectiveType,
    pub activation_event: String,
}
impl ObjectiveDetails {
    pub fn new(id_name: String, objective_type: ObjectiveType, activation_event: String) -> Self {
        Self { id_name, objective_type, activation_event }
    }
}

#[derive(Component, Clone, Debug, EnumString, AsRefStr)]
pub enum ObjectiveState {
    Inactive,
    InProgress,
    Completed,
    Failed,
}

#[derive(Component, SSS)]
pub struct BuilderObjective {
    pub objective_details: ObjectiveDetails,
    /// Saved state. `None` ⇒ fresh spawn (use `Inactive`); `Some` ⇒ restore.
    pub state: Option<ObjectiveState>,
    /// Saved kill-wisps data `(target_amount, started_amount)`.
    /// `None` ⇒ fresh spawn (use stats); `Some` ⇒ restore.
    pub kill_wisps_data: Option<(usize, usize)>,
}
impl BuilderObjective {
    pub fn new(objective_details: ObjectiveDetails) -> Self {
        Self { objective_details, state: None, kill_wisps_data: None }
    }
    pub fn with_state(mut self, state: ObjectiveState) -> Self {
        self.state = Some(state);
        self
    }
    pub fn with_kill_wisps_data(mut self, target_amount: usize, started_amount: usize) -> Self {
        self.kill_wisps_data = Some((target_amount, started_amount));
        self
    }
}

#[derive(Component)]
pub struct ObjectiveCheckmark;
#[derive(Component)]
pub struct ObjectiveText;


#[derive(Component)]
pub struct Objective {
    pub checkmark: Entity,
    pub text: Entity,
}

// ---- SPECIFIC OBJECTIVES ----

#[derive(Component, Default)]
pub struct ObjectiveClearAllQuantumFields {
    pub completed_quantum_fields: usize,
}

#[derive(Component, Default, Clone)]
pub struct ObjectiveKillWisps {
    pub target_amount: usize,
    pub started_amount: usize,
}
