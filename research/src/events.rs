use bevy::prelude::*;

/// Start or switch the active research. Parks the incumbent (back to
/// `Available`, progress retained) and sets the target `Active`. Only an
/// `Available` research can be started — one with no state cannot be targeted.
#[derive(EntityEvent, Clone, Copy, Debug)]
pub struct SetActiveResearch {
    #[event_target]
    pub research: Entity,
}

/// Park an active research: set it back to `Available`, progress retained.
/// Entity-targeted rather than a global "stop whatever is running" — a
/// targeted event that no-ops on a research which is not active is more
/// precise than one that stops whatever happens to be running.
#[derive(EntityEvent, Clone, Copy, Debug)]
pub struct StopResearch {
    #[event_target]
    pub research: Entity,
}

/// Emitted by the tick when a research reaches 1.0. Distinct from inserting
/// `ResearchState::Completed`, which also happens during restore — completion
/// logic (firing outcomes) listens to this, not to the state insert.
#[derive(EntityEvent, Clone, Copy, Debug)]
pub struct ResearchFinished {
    #[event_target]
    pub research: Entity,
}

/// Fired when a research's display data changes — icon resolved, name edited,
/// etc. Tiles listen to this to populate their content.
#[derive(EntityEvent, Clone, Copy, Debug)]
pub struct ResearchDisplayDataUpdated {
    #[event_target]
    pub research: Entity,
}
