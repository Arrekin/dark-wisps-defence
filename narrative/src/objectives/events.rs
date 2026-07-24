use bevy::prelude::*;

use game_core::prelude::FromEntity;

use super::relations::ObjectiveGoalOf;

/// Goal-state-change event. Triggered on the goal when its `ObjectiveState` is
/// inserted. Propagates up `ObjectiveGoalOf` to the objective root, where the
/// aggregator observes it.
///
/// `auto_propagate` = the event always bubbles without observers needing to
/// call `On::propagate(true)`. The custom traversal `&'static ObjectiveGoalOf`
/// is covered by Bevy's blanket `impl<R: Relationship, D> Traversal<D> for &R`
/// (`bevy_ecs-0.19.0/src/traversal.rs:48`).
#[derive(Debug, Clone, EntityEvent, FromEntity)]
#[entity_event(propagate = &'static ObjectiveGoalOf, auto_propagate)]
pub struct ObjectiveGoalStateChanged {
    pub entity: Entity,
}

/// Explicit activation command-event, triggered on an objective root. The ONLY
/// activation entry point (the `MomentHappened` reactor, the editor, dev code).
/// Raw `ObjectiveState` inserts are side-effect-free restoration (loaders restore
/// root and goal states across stages) — activation side effects (propagation to
/// goals, vacuous-satisfy) live only here.
#[derive(Debug, Clone, EntityEvent, FromEntity)]
pub struct ObjectiveActivate {
    pub entity: Entity,
}

/// Terminal event: emitted when an objective root enters `Satisfied`.
/// Named with `Event` suffix to avoid collision with the `ObjectiveSatisfied`
/// marker component.
#[derive(Debug, Clone, EntityEvent, FromEntity)]
pub struct ObjectiveSatisfiedEvent {
    pub entity: Entity,
}

/// Terminal event: emitted when an objective root enters `Failed`.
#[derive(Debug, Clone, EntityEvent, FromEntity)]
pub struct ObjectiveFailedEvent {
    pub entity: Entity,
}
