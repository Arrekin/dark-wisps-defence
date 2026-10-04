use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

use bevy::{
    ecs::{schedule::ScheduleLabel, system::SystemParam},
    input::common_conditions::input_just_released,
    prelude::*,
    tasks::IoTaskPool,
};

use game_core::prelude::{GridCoords, GridImprint};
use logging::prelude::*;

use crate::{
    common::{Migrations, with_db_connection},
    map_file_name::MapFileName,
    map_list::GameMapList,
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
    pub fn register_entity(&self, entity_id: u32) -> rusqlite::Result<usize> {
        self.tx.prepare_cached("INSERT OR IGNORE INTO entities (id) VALUES (?1)")?.execute([entity_id])
    }

    /// Save entity of the object in its dedicated table. Calls register_entity()
    pub fn save_marker(&self, table_name: &str, entity_id: u32) -> rusqlite::Result<usize> {
        self.register_entity(entity_id)?;
        let query = format!("INSERT OR REPLACE INTO {} (id) VALUES (?1)", table_name);
        self.tx.prepare_cached(&query)?.execute([entity_id])
    }

    pub fn save_world_position(&self, entity_id: u32, position: Vec2) -> rusqlite::Result<usize> {
        self.tx.prepare_cached("INSERT INTO world_positions (entity_id, x, y) VALUES (?1, ?2, ?3)")?.execute((entity_id, position.x, position.y))
    }

    pub fn save_integrity_points(&self, entity_id: u32, current: f32) -> rusqlite::Result<usize> {
        self.tx.prepare_cached("INSERT OR REPLACE INTO integrity_points (entity_id, current) VALUES (?1, ?2)")?.execute((entity_id, current))
    }

    pub fn save_disabled_by_player(&self, entity_id: u32) -> rusqlite::Result<usize> {
        self.tx.prepare_cached("INSERT INTO disabled_by_player (entity_id) VALUES (?1)")?.execute([entity_id])
    }

    pub fn save_stat(&self, stat_name: &str, stat_value: f32) -> rusqlite::Result<usize> {
        self.tx.prepare_cached("INSERT OR REPLACE INTO stats (stat_name, stat_value) VALUES (?1, ?2)")?.execute((stat_name, stat_value))
    }

    pub fn save_grid_coords(&self, entity_id: u32, coords: GridCoords) -> rusqlite::Result<usize> {
        self.tx.prepare_cached("INSERT INTO grid_coords (entity_id, x, y) VALUES (?1, ?2, ?3)")?.execute((entity_id, coords.x, coords.y))
    }

    pub fn save_grid_imprint(&self, entity_id: u32, imprint: GridImprint) -> rusqlite::Result<usize> {
        let (shape, width, height) = match imprint {
            GridImprint::Rectangle { width, height } => ("Rectangle", Some(width), Some(height)),
            // Stored as: shape="Plus", width=extents, height=NULL
            GridImprint::Plus { extents } => ("Plus", Some(extents), None),
        };

        self.tx.prepare_cached("INSERT OR REPLACE INTO grid_imprints (id, shape, width, height) VALUES (?1, ?2, ?3, ?4)")?.execute(rusqlite::params![entity_id, shape, width, height])
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
// SAVE SIGNAL + CONTEXT
// ============================================================================

#[derive(Debug, Clone)]
pub enum SaveTarget {
    /// `test_save.dwd` (dev keybind; future: named slots as File(path))
    Quick,
    /// `maps/<file_name>.dwd` + scenario mode (reset playthrough metadata on write)
    Scenario(MapFileName),
}

#[derive(Event, Debug, Clone)]
pub struct SaveGameSignal {
    pub target: SaveTarget,
}

impl SaveGameSignal {
    /// Z always saves to `test_save.dwd`, even after loading `maps/<name>.dwd`.
    #[log_tags(Tag::GameSave)]
    fn emit_quick(mut commands: Commands) {
        #[debug_player("Quicksave requested")]
        commands.trigger(SaveGameSignal { target: SaveTarget::Quick });
    }
}

/// A save in progress. While it exists, new save requests are rejected. Inserted by
/// `on_save_game_signal_do_so`, removed by `finalize_save` once the IO task sets `done`.
#[derive(Resource)]
pub struct SaveRunner {
    path: String,
    /// Collectors reset playthrough state (for example whether game start was emitted).
    pub save_as_scenario: bool,
    done: Arc<AtomicBool>,
    error: Arc<AtomicBool>,
}

/// Starts a save unless one is in progress: resolves the target's path and inserts the
/// `SaveRunner` that drives it.
#[log_tags(Tag::GameSave)]
fn on_save_game_signal_do_so(
    trigger: On<SaveGameSignal>,
    mut commands: Commands,
    save_runner: Option<Res<SaveRunner>>,
) {
    #[warn_player("Save already in flight — skipping")]
    if save_runner.is_some() { return; }
    let target = trigger.event().target.clone();
    let (path, save_as_scenario) = match target {
        SaveTarget::Quick => ("test_save.dwd".to_string(), false),
        SaveTarget::Scenario(file_name) => (file_name.path(), true),
    };
    commands.insert_resource(SaveRunner {
        path,
        save_as_scenario,
        done: Arc::new(AtomicBool::new(false)),
        error: Arc::new(AtomicBool::new(false)),
    });
}

// ============================================================================
// DRIVER
// ============================================================================

/// Exclusive save driver. Runs in `Last` only when `SaveRunner` was added
/// this frame. Collects jobs from the `CollectSave` schedule, hands them to
/// one detached IO task that writes `<path>.tmp` and atomically renames.
#[log_tags(Tag::GameSave)]
fn drive_save(world: &mut World) {
    // 1. Run the collector schedule (collectors read SaveRunner for scenario mode).
    world.run_schedule(CollectSave);

    // 2. Take the jobs the collectors submitted.
    let jobs = std::mem::take(&mut world.resource_mut::<PendingSaveJobs>().0);
    #[warn_dev("SaveGameSignal fired but no jobs were collected — nothing to write")]
    if jobs.is_empty() {
        // Remove the context — nothing to wait for.
        world.remove_resource::<SaveRunner>();
        return;
    }

    // 3. Hand off to a detached IO task.
    let save_runner = world.resource::<SaveRunner>();
    let path = save_runner.path.clone();
    let done = save_runner.done.clone();
    let error = save_runner.error.clone();
    info_dev!("Saving game to '{path}' ({} jobs)", jobs.len());

    IoTaskPool::get()
        .spawn(async move {
            let result = write_save_inner(&path, jobs);
            match result {
                Ok(()) => info_player!("Game saved to '{path}'"),
                #[error_dev("Save failed: {save_error}")]
                Err(save_error) => error.store(true, Ordering::Relaxed),
            }
            done.store(true, Ordering::Relaxed);
        })
        .detach();
}

fn write_save_inner(path: &str, jobs: Vec<SaveJob>) -> Result<(), Box<dyn std::error::Error>> {
    let temporary_path = format!("{path}.tmp");
    if std::path::Path::new(&temporary_path).exists() {
        std::fs::remove_file(&temporary_path)?;
    }

    // Open, migrate, run all jobs in one transaction, then drop the connection
    // before the atomic rename (see `with_db_connection`'s doc comment).
    if let Err(error) = with_db_connection(&temporary_path, Migrations::Apply, |conn| {
        let tx = conn.transaction()?;
        for job in jobs {
            job(&SaveContext { tx: &tx })?;
        }
        tx.commit()?;
        Ok(())
    }) {
        let _ = std::fs::remove_file(&temporary_path);
        return Err(error);
    }

    std::fs::rename(&temporary_path, path)?;
    Ok(())
}

// ============================================================================
// FINALIZE
// ============================================================================

/// Polls `SaveRunner.done` every frame. On completion (success or error),
/// removes `SaveRunner` (reopening the guard). On scenario save, rescans
/// `GameMapList` so the new map appears in the menu without restarting.
#[log_tags(Tag::GameSave)]
fn finalize_save(
    mut commands: Commands,
    save_runner: Res<SaveRunner>,
    mut map_list: ResMut<GameMapList>,
) {
    if !save_runner.done.load(Ordering::Relaxed) { return; }
    if save_runner.error.load(Ordering::Relaxed) { warn_player!("Save failed — context cleared"); }
    if save_runner.save_as_scenario {
        map_list.refresh();
    }
    commands.remove_resource::<SaveRunner>();
}
