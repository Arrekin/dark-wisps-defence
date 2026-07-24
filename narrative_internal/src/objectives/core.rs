use bevy::prelude::*;

use game_core::prelude::{MomentHappened, MomentOfInterest};
use logging::prelude::*;
use narrative::prelude::*;
use persistence::prelude::{GameDbHelpers, LoadContext, SaveContext, SaveWriter};
use persistence::rusqlite;

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
    let mut ec = commands.entity(entity);
    ec.remove::<BuilderObjective>()
        .insert((
            ObjectiveDetails { id_name: builder.id_name.clone() },
            builder.state,
        ));
    if let Some(moment_entity) = builder.activated_by {
        ec.insert(MomentOfInterest(moment_entity));
    }
}

// ============================================================================
// STATE MARKER SYNC
// ============================================================================

/// On every `Insert, ObjectiveState`, swap the marker components to match the
/// new state. Works identically on objectives and goals. Markers are never
/// inserted directly — this is the single entry point that derives them.
pub(crate) fn on_insert_objective_state_sync_markers(
    trigger: On<Insert, ObjectiveState>,
    states: Query<&ObjectiveState>,
    mut commands: Commands,
) {
    let entity = trigger.entity;
    let Ok(new_state) = states.get(entity) else { return };
    let mut ec = commands.entity(entity);
    ec.remove::<(ObjectiveInactive, ObjectiveInProgress, ObjectiveSatisfied, ObjectiveFailed)>();
    match new_state {
        ObjectiveState::Inactive => { ec.insert(ObjectiveInactive); }
        ObjectiveState::InProgress => { ec.insert(ObjectiveInProgress); }
        ObjectiveState::Satisfied => { ec.insert(ObjectiveSatisfied); }
        ObjectiveState::Failed => { ec.insert(ObjectiveFailed); }
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
pub(crate) fn on_objective_activate(
    trigger: On<ObjectiveActivate>,
    objectives: Query<Option<&ObjectiveGoals>, With<ObjectiveDetails>>,
    mut commands: Commands,
) {
    let entity = trigger.entity;
    let Ok(goals) = objectives.get(entity) else { return };
    match goals {
        None => {
            commands.entity(entity)
                .insert(ObjectiveState::Satisfied)
                .trigger(ObjectiveSatisfiedEvent::from);
        }
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
pub(crate) fn on_goal_state_changed_aggregate(
    trigger: On<ObjectiveGoalStateChanged>,
    objectives: Query<&ObjectiveGoals, (With<ObjectiveDetails>, With<ObjectiveInProgress>)>,
    goal_states: Query<&ObjectiveState, With<ObjectiveGoalOf>>,
    mut commands: Commands,
) {
    let root = trigger.entity;
    let Ok(goals) = objectives.get(root) else { return };
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
        commands.entity(root)
            .insert(ObjectiveState::Failed)
            .trigger(ObjectiveFailedEvent::from);
    } else if all_satisfied {
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

pub(crate) fn collect_objectives(
    save_ctx: Res<SaveContext>,
    mut save: SaveWriter,
    objectives: Query<(Entity, &ObjectiveDetails, &ObjectiveState, Option<&MomentOfInterest>)>,
) {
    if objectives.is_empty() { return; }
    let rows: Vec<(i64, String, String, Option<i64>)> = objectives
        .iter()
        .map(|(entity, details, state, activated_by)| {
            let state_str = if save_ctx.save_as_scenario {
                ObjectiveState::Inactive.as_ref().to_string()
            } else {
                state.as_ref().to_string()
            };
            (
                entity.index_u32() as i64,
                details.id_name.clone(),
                state_str,
                activated_by.map(|ab| ab.0.index_u32() as i64),
            )
        })
        .collect();
    save.submit(move |tx| {
        for (id, id_name, state, activated_by) in rows {
            tx.register_entity(id)?;
            tx.execute(
                "INSERT OR REPLACE INTO objectives (id, id_name, state, activated_by) VALUES (?1, ?2, ?3, ?4)",
                rusqlite::params![id, id_name, state, activated_by],
            )?;
        }
        Ok(())
    });
}

pub(crate) fn load_objectives(ctx: &mut LoadContext) -> rusqlite::Result<()> {
    let mut stmt = ctx.conn.prepare("SELECT id, id_name, state, activated_by FROM objectives")?;
    let mut rows = stmt.query([])?;
    while let Some(row) = rows.next()? {
        let old_id: i64 = row.get(0)?;
        let id_name: String = row.get(1)?;
        let state_str: String = row.get(2)?;
        let activated_by: Option<i64> = row.get(3)?;

        let Some(entity) = ctx.entity(old_id) else {
            Log::warn().dev().tag(Tag::GameLoad).message(format!("Objective with old ID {old_id} has no corresponding new entity"));
            continue;
        };

        let Ok(state) = state_str.parse::<ObjectiveState>() else {
            Log::warn().dev().tag(Tag::GameLoad).message(format!("Unknown objective state in save: {state_str}"));
            continue;
        };

        // Lost-activation load rule: an Inactive objective whose activation moment failed
        // remap can never activate — load as Failed. Non-Inactive objectives
        // (Satisfied/InProgress) already activated or completed; their
        // activation moment is irrelevant, so preserve the saved state.
        let (state, activated_by) = if let Some(ab_old_id) = activated_by {
            match ctx.entity(ab_old_id) {
                Some(moment_entity) => (state, Some(moment_entity)),
                None => {
                    if state == ObjectiveState::Inactive {
                        Log::error().dev().tag(Tag::GameLoad).message(format!(
                            "Inactive objective '{id_name}' (old ID {old_id}) has activated_by={ab_old_id} that failed entity remap — loading as Failed"
                        ));
                        (ObjectiveState::Failed, None)
                    } else {
                        Log::warn().dev().tag(Tag::GameLoad).message(format!(
                            "Objective '{id_name}' (old ID {old_id}, state {state_str}) has activated_by={ab_old_id} that failed entity remap — preserving saved state"
                        ));
                        (state, None)
                    }
                }
            }
        } else {
            (state, None)
        };

        let mut builder = BuilderObjective::new(id_name).with_state(state);
        if let Some(moment_entity) = activated_by {
            builder = builder.with_activated_by(moment_entity);
        }
        ctx.insert(entity, builder);
    }
    Ok(())
}
