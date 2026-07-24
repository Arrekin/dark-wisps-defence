use bevy::prelude::*;

/// Source side (on the goal): "this goal belongs to objective N."
/// Linked despawn ON — when the objective despawns, all its goals despawn
/// (via `ObjectiveGoals`'s `linked_spawn` on the target side).
#[derive(Component)]
#[relationship(relationship_target = ObjectiveGoals)]
pub struct ObjectiveGoalOf(pub Entity);

/// Target side (on the objective): collection of goals. `linked_spawn` causes
/// despawn cascade — when the objective despawns, all goals in this collection
/// are despawned too.
#[derive(Component)]
#[relationship_target(relationship = ObjectiveGoalOf, linked_spawn)]
pub struct ObjectiveGoals(Vec<Entity>);
