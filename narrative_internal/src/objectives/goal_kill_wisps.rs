use bevy::prelude::*;
use bevy_egui::egui;

use game_core::prelude::SSS;
use logging::prelude::*;
use narrative::prelude::*;
use persistence::prelude::{AppGameLoadSaveExtension, CollectSave, GameDbHelpers, LoadContext, SaveContext, SaveWriter};
use persistence::rusqlite;
use states::prelude::MapLoadingStage;
use wisps::prelude::WispDied;

pub(crate) struct GoalKillWispsPlugin;
impl Plugin for GoalKillWispsPlugin {
    fn build(&self, app: &mut App) {
        app
            // Editor menu registration + spawn observer
            .register_objective_goal("Kill Wisps", ObjectiveGoalGroup::Goals, BuilderGoalKillWisps::editor_spawn)
            .add_observer(BuilderGoalKillWisps::on_builder_add_spawn_goal_kill_wisps)
            // Runtime observers
            .add_observer(on_wisp_died_increment_kill_wisps)
            .add_observer(on_refresh_kill_wisps_goal)
            // Persistence
            .add_systems(CollectSave, collect_kill_wisps)
            .register_loader(MapLoadingStage::SpawnEffectInstances, "goal_kill_wisps", load_kill_wisps)
            ;
    }
}

/// Config component for "kill N wisps" goals. Editor-authored, always present.
/// `target` is the number of wisp kills required to satisfy this goal.
#[derive(Component, Clone, Debug)]
pub(crate) struct GoalKillWisps {
    pub(crate) target: usize,
}

/// Default kill target for freshly-spawned goals (editor registry hook).
const DEFAULT_KILL_TARGET: usize = 5;

/// Builder for kill-wisps goals. Carries config + restore data.
/// `new(objective, target)` for fresh spawn (editor); `with_*` for restore (load).
/// Runtime counter is always inserted at build time — no activation observer needed.
#[derive(Component, SSS)]
pub(crate) struct BuilderGoalKillWisps {
    objective: Entity,
    target: usize,
    state: ObjectiveState,
    current: usize,
}

impl BuilderGoalKillWisps {
    pub(crate) fn new(objective: Entity, target: usize) -> Self {
        Self { objective, target, state: ObjectiveState::Inactive, current: 0 }
    }
    pub(crate) fn with_state(mut self, state: ObjectiveState) -> Self {
        self.state = state;
        self
    }
    pub(crate) fn with_current(mut self, current: usize) -> Self {
        self.current = current;
        self
    }

    /// Editor registry hook. Adapts `new(objective, target)` with a default
    /// target to the registry's `fn(&mut Commands, Entity)` signature.
    /// Non-editor callers should use `commands.spawn(BuilderGoalKillWisps::new(...))` directly.
    pub(crate) fn editor_spawn(commands: &mut Commands, objective: Entity) {
        commands.spawn(Self::new(objective, DEFAULT_KILL_TARGET));
    }

    fn on_builder_add_spawn_goal_kill_wisps(
        trigger: On<Add, BuilderGoalKillWisps>,
        mut commands: Commands,
        builders: Query<&BuilderGoalKillWisps>,
    ) {
        let entity = trigger.entity;
        let Ok(builder) = builders.get(entity) else { return };
        commands.entity(entity)
            .remove::<BuilderGoalKillWisps>()
            .insert((
                ObjectiveGoalOf(builder.objective),
                GoalKillWisps { target: builder.target },
                builder.state,
                ObjectiveCounterProgress { current: builder.current, total: builder.target },
                ObjectiveDisplayLine::default(),
                ObjectiveEditorUi(ui_kill_wisps),
            ));
        commands.trigger(RefreshKillWispsGoal { goal: entity });
    }
}

// ============================================================================
// REFRESH (recompute display from live config + counter)
// ============================================================================

/// Fired when the goal's config or counter changes outside the normal progress
/// path (e.g., editor edits the target). Recomputes the display line.
#[derive(Event)]
struct RefreshKillWispsGoal {
    goal: Entity,
}

fn on_refresh_kill_wisps_goal(
    trigger: On<RefreshKillWispsGoal>,
    mut goals: Query<(&GoalKillWisps, &ObjectiveCounterProgress, &mut ObjectiveDisplayLine)>,
) {
    let goal = trigger.goal;
    let Ok((config, progress, mut display)) = goals.get_mut(goal) else { return };
    display.0 = format!("Kill {}/{} wisps", progress.current, config.target);
}

// ============================================================================
// PROGRESS
// ============================================================================

/// On `WispDied`: increment counter on all in-progress kill-wisps goals, then
/// refresh display via `RefreshKillWispsGoal`. `WispDied` is a live gameplay
/// event — never fires during load.
fn on_wisp_died_increment_kill_wisps(
    _trigger: On<WispDied>,
    mut commands: Commands,
    mut goals: Query<
        (&mut ObjectiveCounterProgress, Entity),
        (With<GoalKillWisps>, With<ObjectiveGoalOf>, With<ObjectiveInProgress>),
    >,
) {
    for (mut progress, goal) in goals.iter_mut() {
        if progress.increment_and_check() {
            commands.entity(goal)
                .insert(ObjectiveState::Satisfied)
                .trigger(ObjectiveGoalStateChanged::from);
        }
        commands.trigger(RefreshKillWispsGoal { goal });
    }
}

// ============================================================================
// EDITOR UI
// ============================================================================

fn ui_kill_wisps(ui: &mut egui::Ui, entity: &mut EntityWorldMut) {
    let id = entity.id();
    let mut changed = false;
    if let Some(mut goal) = entity.get_mut::<GoalKillWisps>() {
        ui.label("Kill Wisps");
        let response = ui.add(egui::DragValue::new(&mut goal.target).range(1..=1000).prefix("Target: "));
        changed = response.changed();
    }
    if changed {
        entity.world_scope(|world| world.commands().trigger(RefreshKillWispsGoal { goal: id }));
    }
}

// ============================================================================
// PERSISTENCE
// ============================================================================

fn collect_kill_wisps(
    save_ctx: Res<SaveContext>,
    mut save: SaveWriter,
    goals: Query<(Entity, &GoalKillWisps, &ObjectiveState, &ObjectiveCounterProgress, &ObjectiveGoalOf)>,
) {
    if goals.is_empty() { return; }
    let save_as_scenario = save_ctx.save_as_scenario;
    let rows: Vec<(i64, i64, String, usize, usize)> = goals
        .iter()
        .map(|(entity, goal, state, progress, goal_of)| {
            let state_str = if save_as_scenario {
                ObjectiveState::Inactive.as_ref().to_string()
            } else {
                state.as_ref().to_string()
            };
            let current = if save_as_scenario { 0 } else { progress.current };
            (
                entity.index_u32() as i64,
                goal_of.0.index_u32() as i64,
                state_str,
                goal.target,
                current,
            )
        })
        .collect();
    save.submit(move |tx| {
        for (id, objective_id, state, target, current) in rows {
            tx.register_entity(id)?;
            tx.register_entity(objective_id)?;
            tx.execute(
                "INSERT OR REPLACE INTO goal_kill_wisps (id, objective_id, state, target, current) VALUES (?1, ?2, ?3, ?4, ?5)",
                rusqlite::params![id, objective_id, state, target, current],
            )?;
        }
        Ok(())
    });
}

fn load_kill_wisps(ctx: &mut LoadContext) -> rusqlite::Result<()> {
    let mut stmt = ctx.conn.prepare("SELECT id, objective_id, state, target, current FROM goal_kill_wisps")?;
    let mut rows = stmt.query([])?;
    while let Some(row) = rows.next()? {
        let old_id: i64 = row.get(0)?;
        let objective_old_id: i64 = row.get(1)?;
        let state_str: String = row.get(2)?;
        let target: usize = row.get::<_, i64>(3)? as usize;
        let current: usize = row.get::<_, i64>(4)? as usize;

        let Some(entity) = ctx.entity(old_id) else {
            Log::warn().dev().tag(Tag::GameLoad).message(format!("GoalKillWisps with old ID {old_id} has no corresponding new entity"));
            continue;
        };
        let Some(objective_entity) = ctx.entity(objective_old_id) else {
            Log::warn().dev().tag(Tag::GameLoad).message(format!("GoalKillWisps with old ID {old_id} references objective {objective_old_id} that failed remap"));
            continue;
        };
        let Ok(state) = state_str.parse::<ObjectiveState>() else {
            Log::warn().dev().tag(Tag::GameLoad).message(format!("Unknown objective state in save: {state_str}"));
            continue;
        };

        ctx.insert(entity, BuilderGoalKillWisps::new(objective_entity, target)
            .with_state(state)
            .with_current(current));
    }
    Ok(())
}
