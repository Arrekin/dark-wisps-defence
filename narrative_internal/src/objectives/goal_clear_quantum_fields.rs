use bevy::prelude::*;
use bevy_egui::egui;

use game_core::prelude::SSS;
use logging::prelude::*;
use map_objects::prelude::{QuantumField, QuantumFieldSolved};
use narrative::prelude::*;
use persistence::prelude::{AppGameLoadSaveExtension, CollectSave, GameDbHelpers, LoadContext, SaveContext, SaveWriter};
use persistence::rusqlite;
use states::prelude::MapLoadingStage;

pub(crate) struct GoalClearQuantumFieldsPlugin;
impl Plugin for GoalClearQuantumFieldsPlugin {
    fn build(&self, app: &mut App) {
        app
            // Editor menu registration + spawn observer
            .register_objective_goal("Clear Quantum Fields", ObjectiveGoalGroup::Goals, BuilderGoalClearQuantumFields::editor_spawn)
            .add_observer(BuilderGoalClearQuantumFields::on_builder_add_spawn_goal_clear_quantum_fields)
            // Runtime observers
            .add_observer(on_clear_quantum_fields_activated)
            .add_observer(on_add_quantum_field_or_solved_request_refresh)
            .add_observer(on_refresh_clear_quantum_fields_goal)
            // Persistence
            .add_systems(CollectSave, collect_clear_quantum_fields)
            .register_loader(MapLoadingStage::SpawnEffectInstances, "goal_clear_quantum_fields", load_clear_quantum_fields)
            ;
    }
}

/// Config component (marker) for "clear all quantum fields" goals. No config data —
/// the target is determined by counting all `QuantumField` entities on the map.
/// Satisfied when all quantum fields have `QuantumFieldSolved`.
#[derive(Component, Default)]
pub(crate) struct GoalClearQuantumFields;

/// Builder for clear-quantum-fields goals. Carries config + restore data.
/// `new(objective)` for fresh spawn (editor); `with_state` for restore (load).
/// Runtime counter (`ObjectiveCounterProgress`) is NOT restored from save —
/// the builder fires `RefreshClearQuantumFieldsGoal` which recomputes both
/// `current` and `total` from the live world.
#[derive(Component, SSS)]
pub(crate) struct BuilderGoalClearQuantumFields {
    objective: Entity,
    state: ObjectiveState,
}

impl BuilderGoalClearQuantumFields {
    pub(crate) fn new(objective: Entity) -> Self {
        Self { objective, state: ObjectiveState::Inactive }
    }
    pub(crate) fn with_state(mut self, state: ObjectiveState) -> Self {
        self.state = state;
        self
    }

    /// Editor registry hook. Adapts `new(objective)` to the registry's
    /// `fn(&mut Commands, Entity)` signature. Non-editor callers should use
    /// `commands.spawn(BuilderGoalClearQuantumFields::new(...))` directly.
    pub(crate) fn editor_spawn(commands: &mut Commands, objective: Entity) {
        commands.spawn(Self::new(objective));
    }

    fn on_builder_add_spawn_goal_clear_quantum_fields(
        trigger: On<Add, BuilderGoalClearQuantumFields>,
        mut commands: Commands,
        builders: Query<&BuilderGoalClearQuantumFields>,
    ) {
        let entity = trigger.entity;
        let Ok(builder) = builders.get(entity) else { return };
        commands.entity(entity)
            .remove::<BuilderGoalClearQuantumFields>()
            .insert((
                ObjectiveGoalOf(builder.objective),
                GoalClearQuantumFields,
                builder.state,
                ObjectiveCounterProgress::default(),
                ObjectiveDisplayLine::default(),
                ObjectiveEditorUi(ui_clear_quantum_fields),
            ));
        // Recompute counter from the live world. Quantum fields load in
        // `SpawnMapElements` (prior stage), so they exist at this point.
        commands.trigger(RefreshClearQuantumFieldsGoal { goal: entity });
    }
}

// ============================================================================
// REFRESH EVENT
// ============================================================================

/// Sink event: recompute the goal's counter from the live world. Fired by
/// the builder spawn observer, the activation observer, and global
/// `On<Add, QuantumFieldSolved>` / `On<Add, QuantumField>` observers. All actual
/// recomputation logic lives in `on_refresh_clear_quantum_fields_goal`.
#[derive(Event)]
struct RefreshClearQuantumFieldsGoal {
    goal: Entity,
}

/// On `Add<QuantumFieldSolved>` or `Add<QuantumField>`: fire refresh on all
/// in-progress clear-quantum-fields goals. A new field changes `total`; a
/// solved field changes `current`. The refresh observer recomputes counter +
/// display; the satisfaction check is done here (progress-change site).
fn on_add_quantum_field_or_solved_request_refresh(
    _trigger: On<Add, (QuantumField, QuantumFieldSolved)>,
    mut commands: Commands,
    goals: Query<Entity, (With<GoalClearQuantumFields>, With<ObjectiveInProgress>)>,
    quantum_fields: Query<Entity, With<QuantumField>>,
    solved_fields: Query<Entity, (With<QuantumField>, With<QuantumFieldSolved>)>,
) {
    // Satisfaction check: only meaningful when a field is solved (progress),
    // but the trigger fires for both Add<QuantumField> and Add<QuantumFieldSolved>.
    // Re-checking here is cheap and correct for both cases.
    let total = quantum_fields.iter().count();
    let current = solved_fields.iter().count();
    if current >= total && total > 0 {
        for goal in goals.iter() {
            commands.entity(goal)
                .insert(ObjectiveState::Satisfied)
                .trigger(|e| ObjectiveGoalStateChanged { entity: e });
        }
    }
    for goal in goals.iter() {
        commands.trigger(RefreshClearQuantumFieldsGoal { goal });
    }
}

/// Recompute `current` (solved fields) and `total` (all fields) from the live
/// world, update counter + display. Unconditional — whoever fired refresh
/// already decided it's needed. State transitions are handled at the upstream
/// progress-change site (trigger observer) and activation observer.
fn on_refresh_clear_quantum_fields_goal(
    trigger: On<RefreshClearQuantumFieldsGoal>,
    mut goals: Query<(&mut ObjectiveCounterProgress, &mut ObjectiveDisplayLine), With<GoalClearQuantumFields>>,
    quantum_fields: Query<Entity, With<QuantumField>>,
    solved_fields: Query<Entity, (With<QuantumField>, With<QuantumFieldSolved>)>,
) {
    let goal = trigger.goal;
    let Ok((mut progress, mut display)) = goals.get_mut(goal) else { return };
    let total = quantum_fields.iter().count();
    let current = solved_fields.iter().count();
    progress.current = current;
    progress.total = total;
    display.0 = format!("Solve {}/{} quantum fields", current, total);
}

// ============================================================================
// ACTIVATION
// ============================================================================

/// On `ObjectiveActivate` for a clear-quantum-fields goal: fire refresh, then
/// check satisfaction (all fields may already be solved at activation time).
/// `ObjectiveActivate` is only fired on live activation (never during load).
fn on_clear_quantum_fields_activated(
    trigger: On<ObjectiveActivate>,
    mut commands: Commands,
    goals: Query<(), (With<GoalClearQuantumFields>, With<ObjectiveGoalOf>)>,
    quantum_fields: Query<Entity, With<QuantumField>>,
    solved_fields: Query<Entity, (With<QuantumField>, With<QuantumFieldSolved>)>,
) {
    let goal = trigger.entity;
    if !goals.contains(goal) { return; }
    let total = quantum_fields.iter().count();
    let current = solved_fields.iter().count();
    if current >= total && total > 0 {
        commands.entity(goal)
            .insert(ObjectiveState::Satisfied)
            .trigger(|e| ObjectiveGoalStateChanged { entity: e });
    }
    commands.trigger(RefreshClearQuantumFieldsGoal { goal });
}

// ============================================================================
// EDITOR UI
// ============================================================================

fn ui_clear_quantum_fields(ui: &mut egui::Ui, _entity: &mut EntityWorldMut) {
    ui.label("Clear all quantum fields on the map");
}

// ============================================================================
// PERSISTENCE
// ============================================================================

fn collect_clear_quantum_fields(
    save_ctx: Res<SaveContext>,
    mut save: SaveWriter,
    goals: Query<(Entity, &ObjectiveState, &ObjectiveGoalOf), With<GoalClearQuantumFields>>,
) {
    if goals.is_empty() { return; }
    let rows: Vec<(i64, i64, String)> = goals
        .iter()
        .map(|(entity, state, goal_of)| {
            let state_str = if save_ctx.save_as_scenario {
                ObjectiveState::Inactive.as_ref().to_string()
            } else {
                state.as_ref().to_string()
            };
            (
                entity.index_u32() as i64,
                goal_of.0.index_u32() as i64,
                state_str,
            )
        })
        .collect();
    save.submit(move |tx| {
        for (id, objective_id, state) in rows {
            tx.register_entity(id)?;
            tx.register_entity(objective_id)?;
            tx.execute(
                "INSERT OR REPLACE INTO goal_clear_quantum_fields (id, objective_id, state) VALUES (?1, ?2, ?3)",
                rusqlite::params![id, objective_id, state],
            )?;
        }
        Ok(())
    });
}

fn load_clear_quantum_fields(ctx: &mut LoadContext) -> rusqlite::Result<()> {
    let mut stmt = ctx.conn.prepare("SELECT id, objective_id, state FROM goal_clear_quantum_fields")?;
    let mut rows = stmt.query([])?;
    while let Some(row) = rows.next()? {
        let old_id: i64 = row.get(0)?;
        let objective_old_id: i64 = row.get(1)?;
        let state_str: String = row.get(2)?;

        let Some(entity) = ctx.entity(old_id) else {
            Log::warn().dev().tag(Tag::GameLoad).message(format!("GoalClearQuantumFields with old ID {old_id} has no corresponding new entity"));
            continue;
        };
        let Some(objective_entity) = ctx.entity(objective_old_id) else {
            Log::warn().dev().tag(Tag::GameLoad).message(format!("GoalClearQuantumFields with old ID {old_id} references objective {objective_old_id} that failed remap"));
            continue;
        };
        let Ok(state) = state_str.parse::<ObjectiveState>() else {
            Log::warn().dev().tag(Tag::GameLoad).message(format!("Unknown objective state in save: {state_str}"));
            continue;
        };

        ctx.insert(entity, BuilderGoalClearQuantumFields::new(objective_entity).with_state(state));
    }
    Ok(())
}
