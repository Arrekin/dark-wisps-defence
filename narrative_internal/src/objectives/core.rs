use bevy::prelude::*;

use game_core::prelude::{InsertSome, MomentHappened, MomentOfInterest};
use logging::prelude::*;
use narrative::prelude::*;
use persistence::{prelude::*, rusqlite};

// ============================================================================
// BUILDER SPAWN OBSERVER
// ============================================================================

pub(crate) fn on_builder_add_spawn_objective(
    trigger: On<Add, BuilderObjective>,
    mut commands: Commands,
    builders: Query<&BuilderObjective>,
) {
    let entity = trigger.entity;
    let Ok(builder) = builders.get(entity) else { return };
    commands.entity(entity)
        .remove::<BuilderObjective>()
        .insert((
            ObjectiveDetails { id_name: builder.id_name.clone() },
            builder.state,
        ))
        .insert_some(builder.activated_by);
}

// ============================================================================
// STATE MARKER SYNC
// ============================================================================

/// On every `Insert, ObjectiveState`, swap the marker components to match the
/// new state. Works identically on objectives and goals. Markers are never
/// inserted directly — this is the single entry point that derives them.
pub(crate) fn on_insert_objective_state_sync_markers(
    trigger: On<Insert, ObjectiveState>,
    mut commands: Commands,
    states: Query<&ObjectiveState>,
) {
    let entity = trigger.entity;
    let Ok(new_state) = states.get(entity) else { return };
    let mut entity_commands = commands.entity(entity);
    entity_commands.remove::<(ObjectiveInactive, ObjectiveInProgress, ObjectiveSatisfied, ObjectiveFailed)>();
    match new_state {
        ObjectiveState::Inactive => { entity_commands.insert(ObjectiveInactive); }
        ObjectiveState::InProgress => { entity_commands.insert(ObjectiveInProgress); }
        ObjectiveState::Satisfied => { entity_commands.insert(ObjectiveSatisfied); }
        ObjectiveState::Failed => { entity_commands.insert(ObjectiveFailed); }
    }
}

// ============================================================================
// ACTIVATION
// ============================================================================

/// The only activation path. On `ObjectiveActivate` at a root: insert
/// `InProgress` on the root, propagate `InProgress` to all goals and fire
/// `ObjectiveActivate` on each goal (goal-type observers may catch it for
/// goal-specific activation behavior, e.g. polarity overrides or refresh);
/// if zero goals, insert `Satisfied` and fire `ObjectiveSatisfiedEvent`
/// (vacuously satisfied at activation). Raw `ObjectiveState` inserts do NOT
/// activate — they are restoration (load path) and only trigger marker sync.
#[log_tags(Tag::Objectives)]
pub(crate) fn on_objective_activate(
    trigger: On<ObjectiveActivate>,
    mut commands: Commands,
    objectives: Query<(&ObjectiveDetails, Option<&ObjectiveGoals>)>,
) {
    let entity = trigger.entity;
    let Ok((details, goals)) = objectives.get(entity) else { return };
    let id_name = &details.id_name;
    match goals {
        #[info_player("Objective '{id_name}' activated and satisfied: it has no goals")]
        None => {
            commands.entity(entity)
                .insert(ObjectiveState::Satisfied)
                .trigger(ObjectiveSatisfiedEvent::from);
        }
        #[info_player("Objective '{id_name}' activated")]
        Some(goals) => {
            commands.entity(entity).insert(ObjectiveState::InProgress);
            for goal in goals.iter() {
                commands.entity(goal)
                    .insert(ObjectiveState::InProgress)
                    .trigger(ObjectiveActivate::from);
            }
        }
    }
}

// ============================================================================
// AGGREGATION
// ============================================================================

/// Observe `ObjectiveGoalStateChanged` at the objective root (where it
/// propagates to). Re-read ALL sibling goal states at event time. If any goal
/// `Failed` → root `Failed` + fire `ObjectiveFailedEvent`. If all goals
/// `Satisfied` → root `Satisfied` + fire `ObjectiveSatisfiedEvent`.
/// `ObjectiveGoalStateChanged` is only fired on live goal transitions (progress
/// observers, activation observers) — never during load.
#[log_tags(Tag::Objectives)]
pub(crate) fn on_goal_state_changed_aggregate(
    trigger: On<ObjectiveGoalStateChanged>,
    mut commands: Commands,
    objectives: Query<(&ObjectiveDetails, &ObjectiveGoals), With<ObjectiveInProgress>>,
    goal_states: Query<&ObjectiveState, With<ObjectiveGoalOf>>,
) {
    let root = trigger.entity;
    let Ok((details, goals)) = objectives.get(root) else { return };
    let mut all_satisfied = true;
    let mut any_failed = false;
    for goal_entity in goals.iter() {
        let Ok(goal_state) = goal_states.get(goal_entity) else {
            // Unreadable goal (mid-spawn) counts as NOT satisfied — never skip toward Satisfied.
            all_satisfied = false;
            continue;
        };
        match goal_state {
            ObjectiveState::Failed => { any_failed = true; break; }
            ObjectiveState::Satisfied => { /* keep checking */ }
            _ => { all_satisfied = false; }
        }
    }
    if any_failed {
        #[info_player("Objective '{}' failed", details.id_name)]
        commands.entity(root)
            .insert(ObjectiveState::Failed)
            .trigger(ObjectiveFailedEvent::from);
    } else if all_satisfied {
        #[info_player("Objective '{}' satisfied", details.id_name)]
        commands.entity(root)
            .insert(ObjectiveState::Satisfied)
            .trigger(ObjectiveSatisfiedEvent::from);
    }
}

// ============================================================================
// MOMENT-WATCHING REACTOR
// ============================================================================

/// On `MomentHappened` at an objective root: if the objective is `Inactive`,
/// activate it.
pub(crate) fn on_moment_happened_activate(
    trigger: On<MomentHappened>,
    mut commands: Commands,
    objectives: Query<(), (With<ObjectiveDetails>, With<ObjectiveInactive>)>,
) {
    let entity = trigger.entity;
    if !objectives.contains(entity) { return; }
    commands.trigger(ObjectiveActivate { entity });
}

/// Lost-watcher rule: when `MomentOfInterest` is removed from an objective
/// (the watched moment despawned, or the objective itself is despawning during
/// map change), if the objective is still `Inactive`, set it to `Failed` and
/// fire `ObjectiveFailedEvent`. Uses `try_insert` — the observer also fires
/// while the objective itself is being despawned (components still readable),
/// and the queued insert must no-op on a gone entity.
pub(crate) fn on_remove_moment_of_interest_fail_inactive(
    trigger: On<Remove, MomentOfInterest>,
    mut commands: Commands,
    objectives: Query<(), (With<ObjectiveDetails>, With<ObjectiveInactive>)>,
) {
    let entity = trigger.entity;
    if !objectives.contains(entity) { return; }
    commands.entity(entity).try_insert(ObjectiveState::Failed);
    commands.trigger(ObjectiveFailedEvent { entity });
}

// ============================================================================
// PERSISTENCE
// ============================================================================

#[log_tags(Tag::GameSave)]
pub(crate) fn collect_objectives(
    save_runner: Res<SaveRunner>,
    mut save: SaveWriter,
    objectives: Query<(Entity, &ObjectiveDetails, &ObjectiveState, Option<&MomentOfInterest>)>,
) {
    if objectives.is_empty() { return; }
    #[debug_dev("Saving {} objectives", rows.len())]
    let rows: Vec<(u32, String, ObjectiveState, Option<u32>)> = objectives
        .iter()
        .map(|(entity, details, state, activated_by)| {
            let state = if save_runner.save_as_scenario { ObjectiveState::Inactive } else { *state };
            (
                entity.index_u32(),
                details.id_name.clone(),
                state,
                activated_by.map(|moment| moment.0.index_u32()),
            )
        })
        .collect();
    save.submit(move |ctx| {
        for (id, id_name, state, activated_by) in rows {
            ctx.register_entity(id)?;
            ctx.tx.execute(
                "INSERT OR REPLACE INTO objectives (id, id_name, state, activated_by) VALUES (?1, ?2, ?3, ?4)",
                rusqlite::params![id, id_name, state.as_ref(), activated_by],
            )?;
        }
        Ok(())
    });
}

#[log_tags(Tag::GameLoad)]
pub(crate) fn load_objectives(ctx: &mut LoadContext) -> LoadResult {
    ctx.for_each_entity("SELECT id, id_name, state, activated_by FROM objectives", |ctx, old_id, entity, row| {
        let id_name: String = row.get(1)?;
        let state = row.get_parsed::<ObjectiveState>(2)?;
        let activated_by: Option<u32> = row.get(3)?;

        // Lost-activation load rule: an Inactive objective whose activation moment failed
        // remap can never activate — load as Failed. Non-Inactive objectives
        // (Satisfied/InProgress) already activated or completed; their
        // activation moment is irrelevant, so preserve the saved state.
        let (state, activated_by) = if let Some(moment_old_id) = activated_by {
            match ctx.entity(moment_old_id).ok() {
                Some(moment_entity) => (state, Some(moment_entity)),
                #[error_dev("Inactive objective '{id_name}' (old ID {old_id}) has activated_by={moment_old_id} that failed entity remap — loading as Failed")]
                None if state == ObjectiveState::Inactive => (ObjectiveState::Failed, None),
                #[warn_dev("Objective '{id_name}' (old ID {old_id}, state {state:?}) has activated_by={moment_old_id} that failed entity remap — preserving saved state")]
                None => (state, None),
            }
        } else {
            (state, None)
        };

        let builder = BuilderObjective::new(id_name).with_state(state).with_activated_by(activated_by);
        ctx.insert(entity, builder);
        Ok(())
    })
}
