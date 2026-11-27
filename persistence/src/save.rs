use std::sync::{Arc, OnceLock};

use bevy::{
    ecs::{schedule::ScheduleLabel, system::SystemParam},
    input::common_conditions::input_just_released,
    prelude::*,
    tasks::IoTaskPool,
};

use serde::Serialize;

use game_core::prelude::{GridArea, GridImprint, GridPosition, ResponseRequest};
use logging::prelude::*;
use session::TimeBound;
use states::InGame;

use crate::{
    common::{Migrations, with_db_connection},
    map_file::MapFile,
    scenario_list::ScenarioList,
};

pub struct MapSavePlugin;
impl Plugin for MapSavePlugin {
    fn build(&self, app: &mut App) {
        app
            // The schedule must exist even with zero collectors, or
            // `world.run_schedule(CollectSave)` panics.
            .init_schedule(CollectSave)
            .init_resource::<PendingSaveJobs>()
            .add_systems(Update, SaveGameSignal::emit_quick.run_if(input_just_released(KeyCode::KeyZ)))
            .add_systems(Update, finalize_save.run_if(resource_exists::<SaveRunner>))
            .add_systems(Last, drive_save.run_if(resource_added::<SaveRunner>))
            .add_observer(on_save_game_signal_do_so);
    }
}

/// A unit of DB work captured on the main thread and run on the IO thread inside the
/// single save transaction. It owns all its data: no borrows into the World.
pub type SaveJob = Box<dyn FnOnce(&SaveContext) -> rusqlite::Result<()> + Send + Sync + 'static>;

/// What a save job writes through: the save transaction and helpers for the shared tables.
pub struct SaveContext<'a> {
    pub tx: &'a rusqlite::Transaction<'a>,
}
impl SaveContext<'_> {
    pub fn register_entity(&self, entity_id: u32) -> rusqlite::Result<()> {
        self.tx.prepare_cached("INSERT OR IGNORE INTO entities (id) VALUES (?1)")?.execute([entity_id])?;
        Ok(())
    }

    /// Registers the entity and inserts its row into the marker table `table_name`.
    pub fn save_marker(&self, table_name: &str, entity_id: u32) -> rusqlite::Result<()> {
        self.register_entity(entity_id)?;
        let query = format!("INSERT OR REPLACE INTO {table_name} (id) VALUES (?1)");
        self.tx.prepare_cached(&query)?.execute([entity_id])?;
        Ok(())
    }

    pub fn save_world_position(&self, entity_id: u32, position: Vec2) -> rusqlite::Result<()> {
        self.tx.prepare_cached("INSERT INTO world_positions (entity_id, x, y) VALUES (?1, ?2, ?3)")?.execute((entity_id, position.x, position.y))?;
        Ok(())
    }

    pub fn save_integrity_points(&self, entity_id: u32, current: f32) -> rusqlite::Result<()> {
        self.tx.prepare_cached("INSERT OR REPLACE INTO integrity_points (entity_id, current) VALUES (?1, ?2)")?.execute((entity_id, current))?;
        Ok(())
    }

    pub fn save_disabled_by_player(&self, entity_id: u32) -> rusqlite::Result<()> {
        self.tx.prepare_cached("INSERT INTO disabled_by_player (entity_id) VALUES (?1)")?.execute([entity_id])?;
        Ok(())
    }

    pub fn save_stat(&self, stat_name: &str, stat_value: f32) -> rusqlite::Result<()> {
        self.tx.prepare_cached("INSERT OR REPLACE INTO stats (stat_name, stat_value) VALUES (?1, ?2)")?.execute((stat_name, stat_value))?;
        Ok(())
    }

    pub fn save_time_bound(&self, entity_id: u32, time_bound: TimeBound) -> rusqlite::Result<()> {
        self.tx.prepare_cached("INSERT OR REPLACE INTO time_bounds (entity_id, starts_at, ends_at) VALUES (?1, ?2, ?3)")?.execute((entity_id, time_bound.starts_at, time_bound.ends_at))?;
        Ok(())
    }

    pub fn save_grid_position(&self, entity_id: u32, position: GridPosition) -> rusqlite::Result<()> {
        self.tx.prepare_cached("INSERT INTO grid_coords (entity_id, x, y, level) VALUES (?1, ?2, ?3, ?4)")?.execute((entity_id, position.coords.x, position.coords.y, position.level.as_ref()))?;
        Ok(())
    }

    pub fn save_grid_imprint(&self, entity_id: u32, imprint: GridImprint) -> rusqlite::Result<()> {
        let (shape, width, height) = match imprint {
            GridImprint::Rectangle { width, height } => ("Rectangle", Some(width), Some(height)),
            GridImprint::Plus { extents } => ("Plus", Some(extents), None),
        };

        self.tx.prepare_cached("INSERT OR REPLACE INTO grid_imprints (id, shape, width, height) VALUES (?1, ?2, ?3, ?4)")?.execute(rusqlite::params![entity_id, shape, width, height])?;
        Ok(())
    }

    /// Saves the origin and level to `grid_coords` and the imprint to `grid_imprints`.
    pub fn save_grid_area(&self, entity_id: u32, area: GridArea) -> rusqlite::Result<()> {
        self.save_grid_position(entity_id, area.origin)?;
        self.save_grid_imprint(entity_id, area.imprint)?;
        Ok(())
    }
}

/// Custom schedule. Domains add collector systems to it; it is ONLY executed by the
/// save driver via `world.run_schedule(CollectSave)`. Never add it to the main loop.
#[derive(ScheduleLabel, Debug, Clone, PartialEq, Eq, Hash)]
pub struct CollectSave;

/// Accumulates jobs during one CollectSave run. Drained by the driver the same frame.
#[derive(Resource, Default)]
pub(crate) struct PendingSaveJobs(pub Vec<SaveJob>);

/// The one way collectors submit work. Wraps Commands so collectors stay parallel;
/// the push lands in `PendingSaveJobs` when command buffers apply (guaranteed before
/// `run_schedule` returns, at the schedule's final sync point).
#[derive(SystemParam)]
pub struct SaveWriter<'w, 's> {
    commands: Commands<'w, 's>,
}
impl SaveWriter<'_, '_> {
    pub fn submit(
        &mut self,
        job: impl FnOnce(&SaveContext) -> rusqlite::Result<()> + Send + Sync + 'static,
    ) {
        self.commands.queue(QueueSaveJob(Box::new(job)));
    }
}

struct QueueSaveJob(SaveJob);
impl Command for QueueSaveJob {
    type Out = ();
    fn apply(self, world: &mut World) {
        world.resource_mut::<PendingSaveJobs>().0.push(self.0);
    }
}

// ============================================================================
// SAVE SIGNAL + RUNNER
// ============================================================================

/// Requests a save to `file`; if a response is requested, reports rejection or completion.
#[derive(Event, Clone)]
pub struct SaveGameSignal {
    pub file: MapFile,
    pub response: ResponseRequest,
}
impl SaveGameSignal {
    pub fn new(file: MapFile) -> Self {
        Self { file, response: ResponseRequest::not_needed() }
    }

    pub fn with_response(mut self, response: ResponseRequest) -> Self {
        self.response = response;
        self
    }

    /// Z always saves to the quicksave, even after loading another file.
    #[log_tags(Tag::GameSave)]
    fn emit_quick(mut commands: Commands) {
        #[debug_player("Quicksave requested")]
        commands.trigger(SaveGameSignal::new(MapFile::Quick));
    }
}

/// Outcome of a `SaveGameSignal`: sent once: on rejection, or when writing succeeds or fails.
#[derive(EntityEvent, Serialize)]
pub struct SaveGameReport {
    #[serde(skip)]
    pub entity: Entity,
    pub result: SaveGameResult,
}

#[derive(Serialize)]
pub enum SaveGameResult {
    /// The file is written in place.
    Saved { file: MapFile },
    /// Another save is in progress.
    AlreadySaving,
    /// No map is in play, or one is still loading.
    NotInGame,
    /// No collector submitted anything to write; the file is unchanged.
    NothingToSave,
    /// Writing failed; the file is unchanged.
    Failed { file: MapFile, reason: String },
}

/// A save in progress. While it exists, new save requests are rejected. Inserted by
/// `on_save_game_signal_do_so`, removed by `finalize_save` once the IO task sets `outcome`.
#[derive(Resource)]
pub struct SaveRunner {
    file: MapFile,
    response: ResponseRequest,
    /// Set once by the IO task: `Err` carries why writing failed.
    outcome: Arc<OnceLock<Result<(), String>>>,
}
impl SaveRunner {
    /// Whether this save writes a scenario: collectors reset playthrough state (for example
    /// whether game start was emitted).
    pub fn saves_as_scenario(&self) -> bool {
        self.file.is_scenario()
    }
}

/// Starts a save while a map is in play and no other save is in progress, by inserting the
/// `SaveRunner` that drives it.
#[log_tags(Tag::GameSave)]
fn on_save_game_signal_do_so(
    trigger: On<SaveGameSignal>,
    mut commands: Commands,
    in_game: Option<Res<State<InGame>>>,
    save_runner: Option<Res<SaveRunner>>,
) {
    let SaveGameSignal { file, response } = trigger.event().clone();
    #[warn_player("No map in play — skipping save")]
    if in_game.is_none() {
        response.report(&mut commands, |entity| SaveGameReport { entity, result: SaveGameResult::NotInGame });
        return;
    }
    #[warn_player("Save already in flight — skipping")]
    if save_runner.is_some() {
        response.report(&mut commands, |entity| SaveGameReport { entity, result: SaveGameResult::AlreadySaving });
        return;
    }
    commands.insert_resource(SaveRunner { file, response, outcome: default() });
}

// ============================================================================
// DRIVER
// ============================================================================

/// Exclusive save driver. Runs in `Last` only when `SaveRunner` was added
/// this frame. Collects jobs from the `CollectSave` schedule, hands them to
/// one detached IO task that writes `<path>.tmp` and atomically renames.
#[log_tags(Tag::GameSave)]
fn drive_save(world: &mut World) {
    world.run_schedule(CollectSave);
    let jobs = std::mem::take(&mut world.resource_mut::<PendingSaveJobs>().0);
    #[warn_dev("SaveGameSignal fired but no jobs were collected — nothing to write")]
    if jobs.is_empty() {
        let save_runner = world.remove_resource::<SaveRunner>().expect("drive_save runs only with a SaveRunner");
        save_runner.response.report(&mut world.commands(), |entity| SaveGameReport { entity, result: SaveGameResult::NothingToSave });
        return;
    }

    let save_runner = world.resource::<SaveRunner>();
    let path = save_runner.file.path();
    let outcome = save_runner.outcome.clone();
    info_dev!("Saving game to '{path}' ({} jobs)", jobs.len());

    IoTaskPool::get()
        .spawn(async move { let _ = outcome.set(write_save(&path, jobs).map_err(|error| error.to_string())); })
        .detach();
}

fn write_save(path: &str, jobs: Vec<SaveJob>) -> Result<(), Box<dyn std::error::Error>> {
    if let Some(directory) = std::path::Path::new(path).parent() {
        std::fs::create_dir_all(directory)?;
    }
    let temporary_path = format!("{path}.tmp");
    if std::path::Path::new(&temporary_path).exists() {
        std::fs::remove_file(&temporary_path)?;
    }

    // Open, migrate, run all jobs in one transaction, then drop the connection
    // before the atomic rename (see `with_db_connection`'s doc comment).
    let written = with_db_connection(&temporary_path, Migrations::Apply, |conn| {
        let tx = conn.transaction()?;
        for job in jobs {
            job(&SaveContext { tx: &tx })?;
        }
        tx.commit()?;
        Ok(())
    });
    if let Err(error) = written {
        let _ = std::fs::remove_file(&temporary_path);
        return Err(error);
    }

    std::fs::rename(&temporary_path, path)?;
    Ok(())
}

// ============================================================================
// FINALIZE
// ============================================================================

/// Polls `SaveRunner.outcome` every frame. On completion (success or error), reports the outcome and
/// removes `SaveRunner` (reopening the guard). On scenario save, rescans `ScenarioList` so the new
/// map appears in the menu without restarting.
#[log_tags(Tag::GameSave)]
fn finalize_save(
    mut commands: Commands,
    save_runner: Res<SaveRunner>,
    mut scenario_list: ResMut<ScenarioList>,
) {
    let Some(outcome) = save_runner.outcome.get() else { return; };
    let file = save_runner.file.clone();
    let result = match outcome {
        #[info_player("Game saved to '{}'", save_runner.file.path())]
        Ok(()) => SaveGameResult::Saved { file },
        #[error_player("Save to '{}' failed: {reason}", save_runner.file.path())]
        Err(reason) => SaveGameResult::Failed { file, reason: reason.clone() },
    };
    save_runner.response.report(&mut commands, |entity| SaveGameReport { entity, result });
    if save_runner.saves_as_scenario() {
        scenario_list.refresh();
    }
    commands.remove_resource::<SaveRunner>();
}
