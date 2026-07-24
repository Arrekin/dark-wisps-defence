use bevy::prelude::*;
use bevy_egui::egui;
use game_core::prelude::{MapBound, SSS};
use strum::{AsRefStr, EnumString};

/// Builder for objective roots. Carries config + restore data.
/// `new(id_name)` for fresh spawn (editor); `with_*` for restore (load).
/// The spawn observer (`on_builder_add_spawn_objective`) lives in `narrative_internal`.
#[derive(Component, SSS)]
pub struct BuilderObjective {
    pub id_name: String,
    pub state: ObjectiveState,
    pub activated_by: Option<Entity>,
}

impl BuilderObjective {
    pub fn new(id_name: String) -> Self {
        Self { id_name, state: ObjectiveState::Inactive, activated_by: None }
    }
    pub fn with_state(mut self, state: ObjectiveState) -> Self {
        self.state = state;
        self
    }
    pub fn with_activated_by(mut self, entity: Entity) -> Self {
        self.activated_by = Some(entity);
        self
    }
}

/// Config component on the objective root. Editor-authored identity/label.
#[derive(Component, Clone, Debug)]
#[require(MapBound)]
pub struct ObjectiveDetails {
    pub id_name: String,
}

/// State enum — single entry point for state changes. Inserting this fires
/// `On<Insert, ObjectiveState>` which swaps markers. Persisted as a string
/// (strum derives for DB serialization).
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq, Default, EnumString, AsRefStr)]
pub enum ObjectiveState {
    #[default]
    Inactive,
    InProgress,
    Satisfied,
    Failed,
}

// ---- Marker components (query-filter surface; never inserted directly) ----

#[derive(Component, Default)]
pub struct ObjectiveInactive;
#[derive(Component, Default)]
pub struct ObjectiveInProgress;
#[derive(Component, Default)]
pub struct ObjectiveSatisfied;
#[derive(Component, Default)]
pub struct ObjectiveFailed;

/// Written by goal logic, read by HUD. On the goal entity.
#[derive(Component, Clone, Debug, Default)]
pub struct ObjectiveDisplayLine(pub String);

/// Generic N-of-M counter shape on goals. Goal observers drive `current` via
/// `increment_and_check()` and transition to `Satisfied` when it returns `true`.
/// Inserted at build time (fresh = 0/target, restore = saved values).
#[derive(Component, Clone, Debug, Default)]
pub struct ObjectiveCounterProgress {
    pub current: usize,
    pub total: usize,
}

impl ObjectiveCounterProgress {
    /// Advance the counter. Returns `true` if the threshold is met.
    pub fn increment_and_check(&mut self) -> bool {
        self.current += 1;
        self.current >= self.total
    }
}

/// Fn-pointer component on goals, attached by each goal module's
/// `On<Add, ConfigComponent>` observer. Always present (no gating — editor is
/// player-facing). Called by the editor's exclusive system with a cloned
/// `egui::Context` + `&mut EntityWorldMut` for the goal.
///
/// Lifetime annotations: three independent lifetimes so the caller can pass
/// `&'short mut EntityWorldMut<'world>` (borrow shorter than the world).
/// The naive `fn(&mut egui::Ui, &mut EntityWorldMut)` signature is imprecise
/// about `EntityWorldMut`'s required lifetime parameter.
#[derive(Component)]
pub struct ObjectiveEditorUi(
    pub for<'a, 'b, 'w> fn(&'a mut egui::Ui, &'b mut EntityWorldMut<'w>),
);
