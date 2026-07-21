use bevy::prelude::*;
use bevy_egui::egui;

use game_core::prelude::SSS;
use logging::prelude::*;
use narrative::prelude::*;
use persistence::prelude::*;
use persistence::rusqlite;
use states::prelude::{GameState, MapLoadingStage};

pub(crate) struct RestrictionTimeAllowancePlugin;
impl Plugin for RestrictionTimeAllowancePlugin {
    fn build(&self, app: &mut App) {
        app
            // Editor menu registration + spawn observer
            .register_objective_goal("Time Allowance", ObjectiveGoalGroup::Restrictions, BuilderRestrictionTimeAllowance::editor_spawn)
            .add_observer(BuilderRestrictionTimeAllowance::on_builder_add_spawn_time_allowance)
            // Runtime observers + tick
            .add_observer(on_time_allowance_activated)
            .add_observer(on_refresh_time_allowance)
            .add_systems(Update, tick_time_allowance.run_if(in_state(GameState::Running)))
            // Persistence
            .add_systems(CollectSave, collect_time_allowance)
            .register_loader(MapLoadingStage::SpawnEffectInstances, "restriction_time_allowance", load_time_allowance)
            ;
    }
}

/// Config component for "time allowance" restrictions. Editor-authored, always
/// present. `seconds` is the time budget after activation before the goal fails.
#[derive(Component, Clone, Debug)]
pub(crate) struct RestrictionTimeAllowance {
    pub(crate) seconds: f32,
}

/// Default time budget (seconds) for freshly-spawned goals (editor registry hook).
const DEFAULT_TIME_ALLOWANCE_SECONDS: f32 = 60.0;

/// Runtime component for time allowance goals. Inserted at build time
/// (fresh = 0.0, restore = saved elapsed). Tracks elapsed seconds since
/// activation. When `elapsed >= seconds`, the goal flips to `Failed`.
#[derive(Component, Clone, Debug, Default)]
pub(crate) struct TimeAllowanceRuntime {
    pub(crate) elapsed: f32,
}

/// Builder for time-allowance goals. Carries config + restore data.
/// `new(objective, seconds)` for fresh spawn (editor); `with_*` for restore (load).
/// Runtime component is always inserted at build time — no activation observer needed for it.
#[derive(Component, SSS)]
pub(crate) struct BuilderRestrictionTimeAllowance {
    objective: Entity,
    seconds: f32,
    state: ObjectiveState,
    elapsed: f32,
}

impl BuilderRestrictionTimeAllowance {
    pub(crate) fn new(objective: Entity, seconds: f32) -> Self {
        Self { objective, seconds, state: ObjectiveState::Inactive, elapsed: 0.0 }
    }
    pub(crate) fn with_state(mut self, state: ObjectiveState) -> Self {
        self.state = state;
        self
    }
    pub(crate) fn with_elapsed(mut self, elapsed: f32) -> Self {
        self.elapsed = elapsed;
        self
    }

    /// Editor registry hook. Adapts `new(objective, seconds)` with a default
    /// seconds to the registry's `fn(&mut Commands, Entity)` signature.
    /// Non-editor callers should use `commands.spawn(BuilderRestrictionTimeAllowance::new(...))` directly.
    pub(crate) fn editor_spawn(commands: &mut Commands, objective: Entity) {
        commands.spawn(Self::new(objective, DEFAULT_TIME_ALLOWANCE_SECONDS));
    }

    fn on_builder_add_spawn_time_allowance(
        trigger: On<Add, BuilderRestrictionTimeAllowance>,
        mut commands: Commands,
        builders: Query<&BuilderRestrictionTimeAllowance>,
    ) {
        let entity = trigger.entity;
        let Ok(builder) = builders.get(entity) else { return };
        commands.entity(entity)
            .remove::<BuilderRestrictionTimeAllowance>()
            .insert((
                ObjectiveGoalOf(builder.objective),
                RestrictionTimeAllowance { seconds: builder.seconds },
                builder.state,
                TimeAllowanceRuntime { elapsed: builder.elapsed },
                ObjectiveDisplayLine::default(),
                ObjectiveEditorUi(ui_time_allowance),
            ));
        commands.trigger(RefreshTimeAllowance { goal: entity });
    }
}

// ============================================================================
// REFRESH (recompute display from live config + runtime)
// ============================================================================

/// Fired when the goal's config or runtime changes outside the normal tick
/// path (e.g., editor edits the seconds). Recomputes the display line.
#[derive(Event)]
struct RefreshTimeAllowance {
    goal: Entity,
}

fn on_refresh_time_allowance(
    trigger: On<RefreshTimeAllowance>,
    mut goals: Query<(&RestrictionTimeAllowance, &TimeAllowanceRuntime, &mut ObjectiveDisplayLine)>,
) {
    let goal = trigger.goal;
    let Ok((config, runtime, mut display)) = goals.get_mut(goal) else { return };
    display.0 = format_remaining(config.seconds - runtime.elapsed);
}

fn format_remaining(seconds: f32) -> String {
    let total = seconds.max(0.0) as u32;
    let mins = total / 60;
    let secs = total % 60;
    format!("{}:{:02} left", mins, secs)
}

// ============================================================================
// ACTIVATION (polarity override: InProgress → Satisfied)
// ============================================================================

/// On `ObjectiveActivate` for a time allowance goal: override to `Satisfied`
/// (maintenance polarity — "within allowance") and fire `ObjectiveGoalStateChanged`
/// so the aggregator can evaluate the root. Runtime component + display line are
/// already set at build time. The `Satisfied` insert does NOT re-trigger this
/// observer (it listens to `ObjectiveActivate`, not `On<Insert, ObjectiveState>`).
fn on_time_allowance_activated(
    trigger: On<ObjectiveActivate>,
    mut commands: Commands,
    goals: Query<(), With<RestrictionTimeAllowance>>,
) {
    let goal = trigger.entity;
    if !goals.contains(goal) { return; }
    commands.entity(goal)
        .insert(ObjectiveState::Satisfied)
        .trigger(|e| ObjectiveGoalStateChanged { entity: e });
}

// ============================================================================
// PER-FRAME TICK (countdown + Failed on expiry)
// ============================================================================

/// Ticks elapsed time on `Satisfied` time allowance goals whose owner objective is
/// still `InProgress`. Freezes when the owner locks (Satisfied/Failed) — the moment
/// of expiry never arrives for a completed objective. Also correct for loaded
/// already-resolved objectives. Display update is delegated to `RefreshTimeAllowance`.
fn tick_time_allowance(
    mut commands: Commands,
    time: Res<Time>,
    mut goals: Query<
        (&RestrictionTimeAllowance, &mut TimeAllowanceRuntime, &ObjectiveGoalOf, Entity),
        With<ObjectiveSatisfied>,
    >,
    active_roots: Query<(), (With<ObjectiveDetails>, With<ObjectiveInProgress>)>,
) {
    let dt = time.delta_secs();
    for (config, mut runtime, goal_of, goal) in goals.iter_mut() {
        if !active_roots.contains(goal_of.0) { continue; }
        runtime.elapsed += dt;
        commands.trigger(RefreshTimeAllowance { goal });
        if runtime.elapsed >= config.seconds {
            commands.entity(goal)
                .insert(ObjectiveState::Failed)
                .trigger(|e| ObjectiveGoalStateChanged { entity: e });
        }
    }
}

// ============================================================================
// EDITOR UI
// ============================================================================

fn ui_time_allowance(ui: &mut egui::Ui, entity: &mut EntityWorldMut) {
    let id = entity.id();
    let mut changed = false;
    if let Some(mut config) = entity.get_mut::<RestrictionTimeAllowance>() {
        ui.label("Time Allowance");
        let response = ui.add(egui::DragValue::new(&mut config.seconds).range(1.0..=3600.0).speed(0.1).prefix("Seconds: "));
        changed = response.changed();
    }
    if changed {
        entity.world_scope(|world| world.commands().trigger(RefreshTimeAllowance { goal: id }));
    }
}

// ============================================================================
// PERSISTENCE
// ============================================================================

fn collect_time_allowance(
    save_ctx: Res<SaveContext>,
    mut save: SaveWriter,
    goals: Query<(Entity, &RestrictionTimeAllowance, &ObjectiveState, &TimeAllowanceRuntime, &ObjectiveGoalOf)>,
) {
    if goals.is_empty() { return; }
    let save_as_scenario = save_ctx.save_as_scenario;
    let rows: Vec<(i64, i64, String, f32, f32)> = goals
        .iter()
        .map(|(entity, config, state, runtime, goal_of)| {
            let state_str = if save_as_scenario {
                ObjectiveState::Inactive.as_ref().to_string()
            } else {
                state.as_ref().to_string()
            };
            let elapsed = if save_as_scenario { 0.0 } else { runtime.elapsed };
            (
                entity.index_u32() as i64,
                goal_of.0.index_u32() as i64,
                state_str,
                config.seconds,
                elapsed,
            )
        })
        .collect();
    save.submit(move |tx| {
        for (id, objective_id, state, seconds, elapsed) in rows {
            tx.register_entity(id)?;
            tx.register_entity(objective_id)?;
            tx.execute(
                "INSERT OR REPLACE INTO restriction_time_allowance (id, objective_id, state, seconds, elapsed) VALUES (?1, ?2, ?3, ?4, ?5)",
                rusqlite::params![id, objective_id, state, seconds, elapsed],
            )?;
        }
        Ok(())
    });
}

fn load_time_allowance(ctx: &mut LoadContext) -> rusqlite::Result<()> {
    let mut stmt = ctx.conn.prepare("SELECT id, objective_id, state, seconds, elapsed FROM restriction_time_allowance")?;
    let mut rows = stmt.query([])?;
    while let Some(row) = rows.next()? {
        let old_id: i64 = row.get(0)?;
        let objective_old_id: i64 = row.get(1)?;
        let state_str: String = row.get(2)?;
        let seconds: f32 = row.get(3)?;
        let elapsed: f32 = row.get(4)?;

        let Some(entity) = ctx.entity(old_id) else {
            Log::warn().dev().tag(Tag::GameLoad).message(format!("RestrictionTimeAllowance with old ID {old_id} has no corresponding new entity"));
            continue;
        };
        let Some(objective_entity) = ctx.entity(objective_old_id) else {
            Log::warn().dev().tag(Tag::GameLoad).message(format!("RestrictionTimeAllowance with old ID {old_id} references objective {objective_old_id} that failed remap"));
            continue;
        };
        let Ok(state) = state_str.parse::<ObjectiveState>() else {
            Log::warn().dev().tag(Tag::GameLoad).message(format!("Unknown objective state in save: {state_str}"));
            continue;
        };

        ctx.insert(entity, BuilderRestrictionTimeAllowance::new(objective_entity, seconds)
            .with_state(state)
            .with_elapsed(elapsed));
    }
    Ok(())
}
