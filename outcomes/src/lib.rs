use bevy::prelude::*;
use bevy_egui::egui;

// ============================================================================
// Outcomes
//
// An outcome is a satellite entity attached to a parent that, on a trigger,
// releases whatever it holds. The owning domain fires `FulfillOutcome` on
// the outcome entity when the parent's condition is met; the outcome kind
// self-observes it and acts. The generic layer stops at delivering the event.
//
// Outcome kinds are distributed across domains: each domain defines its own
// outcome component, spawns outcome children on its entities, and self-observes
// `FulfillOutcome`.
// ============================================================================

/// Source side (on the outcome): "this outcome belongs to parent P."
/// Linked despawn ON — outcomes die with their parent. When the parent
/// despawns (e.g. on map unload), `linked_spawn` cascades the despawn to
/// all its outcomes.
#[derive(Component)]
#[relationship(relationship_target = HasOutcomes)]
pub struct OutcomeOf(pub Entity);

/// Target side (on the parent): collection of outcomes owned by this entity.
/// `linked_spawn` causes despawn cascade — when the parent despawns, all its
/// outcomes despawn too.
#[derive(Component)]
#[relationship_target(relationship = OutcomeOf, linked_spawn)]
pub struct HasOutcomes(Vec<Entity>);

/// Fired on an outcome entity when its parent's condition is met. The outcome
/// kind self-observes this and acts (e.g. unlock a blueprint, grant a resource).
/// The generic layer does not fire this — the parent's domain does, on its own
/// terms.
#[derive(Debug, Clone, Copy, EntityEvent)]
pub struct FulfillOutcome {
    #[event_target]
    pub outcome: Entity,
}

// ============================================================================
// Outcome kind registry — feeds the editor's "Add Outcome" menu.
// Populated by each outcome kind's plugin via `register_outcome_kind`.
// ============================================================================

/// Registry of available outcome kinds. Mirrors `ObjectiveGoalRegistry`.
#[derive(Resource, Default, Clone)]
pub struct OutcomeKindRegistry {
    pub entries: Vec<OutcomeKindEntry>,
}

/// One entry per outcome kind. `spawn` creates an outcome entity with the
/// kind's config component + `OutcomeOf(parent)` on the given parent entity.
#[derive(Clone, Copy)]
pub struct OutcomeKindEntry {
    pub name: &'static str,
    pub spawn: fn(&mut Commands, Entity),
}

/// App extension for registering outcome kinds.
pub trait AppOutcomeKindExt {
    fn register_outcome_kind(&mut self, name: &'static str, spawn: fn(&mut Commands, Entity)) -> &mut Self;
}

impl AppOutcomeKindExt for App {
    fn register_outcome_kind(&mut self, name: &'static str, spawn: fn(&mut Commands, Entity)) -> &mut Self {
        self.init_resource::<OutcomeKindRegistry>();
        self.world_mut()
            .resource_mut::<OutcomeKindRegistry>()
            .entries
            .push(OutcomeKindEntry { name, spawn });
        self
    }
}

// ============================================================================
// Outcome editor UI — per-kind config drawn by a fn pointer on the outcome.
// ============================================================================

/// Per-kind editor UI fn pointer, stored as a component on each outcome.
/// The editor tab calls it to draw the kind's configuration (e.g. a
/// `ShardType` dropdown for `UnlockShardBlueprint`).
///
/// HRTB lifetime: `for<'a, 'b, 'w> fn(&'a mut egui::Ui, &'b mut EntityWorldMut<'w>)`
/// — same pattern as `ObjectiveEditorUi`, needed because `EntityWorldMut`
/// carries a lifetime parameter that the naive signature can't express.
#[derive(Component)]
pub struct OutcomeEditorUi(
    pub for<'a, 'b, 'w> fn(&'a mut egui::Ui, &'b mut EntityWorldMut<'w>),
);

pub mod prelude {
    pub use crate::{
        AppOutcomeKindExt, FulfillOutcome, HasOutcomes, OutcomeEditorUi, OutcomeKindEntry,
        OutcomeKindRegistry, OutcomeOf,
    };
}
