use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use bevy::{
    ecs::schedule::ScheduleLabel,
    ecs::system::SystemParam,
    input::common_conditions::input_just_released,
    prelude::*,
    tasks::IoTaskPool,
};

use logging::prelude::*;

use crate::common::{db_migrations, with_db_connection};

pub struct MapSavePlugin;
impl Plugin for MapSavePlugin {
    fn build(&self, app: &mut App) {
        app
            // Schedule must EXIST even with zero collectors (T1 has none), or
            // `world.run_schedule(CollectSave)` panics.
            .init_schedule(CollectSave)
            .init_resource::<PendingSaveJobs>()
            .init_resource::<ActiveSaveFile>()
            .init_resource::<SaveInFlight>()
            .add_message::<SaveGameSignal>()
            .add_systems(Update, (
                SaveGameSignal::emit.run_if(input_just_released(KeyCode::KeyZ)),
            ))
            .add_systems(Last, (
                drive_save.run_if(on_message::<SaveGameSignal>),
            ))
            ;
    }
}

/// A unit of DB work captured on the main thread, executed on the IO thread inside
/// the single save transaction. Must own all its data (no borrows into the World).
/// `Send + Sync + 'static` (the expansion of `SSS`) is used instead of bare `+ Send`
/// so that `PendingSaveJobs` satisfies Bevy 0.19's `#[derive(Resource)]` bound.
/// (`SSS` itself can't appear in `dyn` syntax — only auto traits can.)
pub type SaveJob = Box<dyn FnOnce(&rusqlite::Transaction) -> rusqlite::Result<()> + Send + Sync + 'static>;

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
        job: impl FnOnce(&rusqlite::Transaction) -> rusqlite::Result<()> + Send + Sync + 'static,
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

/// Path of the .dwd file the running session saves to / was loaded from.
/// Overwritten by the load observer (map_path) and by the dev save keybind.
/// Default exists so `drive_save` can never panic.
#[derive(Resource)]
pub struct ActiveSaveFile(pub String);
impl Default for ActiveSaveFile {
    fn default() -> Self {
        Self("test_save.dwd".into())
    }
}

/// True while the IO task is writing. Driver skips (with a player-visible warn log)
/// if a save is already in flight.
#[derive(Resource, Default)]
pub(crate) struct SaveInFlight(pub Arc<AtomicBool>);

#[derive(Message)]
pub(crate) struct SaveGameSignal;
impl SaveGameSignal {
    /// Z always saves to `test_save.dwd`, even after loading `maps/<name>.dwd`.
    fn emit(mut writer: MessageWriter<SaveGameSignal>, mut path: ResMut<ActiveSaveFile>) {
        path.0 = "test_save.dwd".into();
        writer.write(SaveGameSignal);
    }
}

// --- Driver ------------------------------------------------------------------

/// Exclusive save driver. Runs in `Last` only when a `SaveGameSignal` was emitted
/// this frame. Collects jobs from the `CollectSave` schedule, hands them to one
/// detached IO task that writes `<path>.tmp` and atomically renames over `<path>`.
fn drive_save(world: &mut World) {
    // 1. In-flight guard.
    if world.resource::<SaveInFlight>().0.load(Ordering::Relaxed) {
        Log::warn()
            .player()
            .tag(Tag::GameSave)
            .message("Save already in flight — skipping");
        return;
    }

    // 2. Run the collector schedule.
    world.run_schedule(CollectSave);

    // 3. Take the job buffer. The emptiness check is on this taken vec, AFTER
    //    run_schedule — never a pre-check before it.
    let jobs = std::mem::take(&mut world.resource_mut::<PendingSaveJobs>().0);
    if jobs.is_empty() {
        Log::warn()
            .dev()
            .tag(Tag::GameSave)
            .message("SaveGameSignal fired but no jobs were collected — nothing to write");
        return;
    }

    // 4. Hand off to a detached IO task.
    let path = world.resource::<ActiveSaveFile>().0.clone();
    let in_flight = world.resource::<SaveInFlight>().0.clone();
    in_flight.store(true, Ordering::Relaxed);
    Log::info()
        .dev()
        .tag(Tag::GameSave)
        .message(format!("Saving game to '{path}' ({} jobs)", jobs.len()));

    IoTaskPool::get()
        .spawn(async move {
            write_save(path, jobs, in_flight);
        })
        .detach();
}

fn write_save(path: String, jobs: Vec<SaveJob>, in_flight: Arc<AtomicBool>) {
    let result = write_save_inner(&path, jobs);
    match result {
        Ok(()) => {
            Log::info()
                .player()
                .tag(Tag::GameSave)
                .message(format!("Game saved to '{path}'"));
        }
        Err(e) => {
            Log::error()
                .dev()
                .tag(Tag::GameSave)
                .message(format!("Save failed: {e}"));
        }
    }
    // ALWAYS clear the in-flight flag, also on error.
    in_flight.store(false, Ordering::Relaxed);
}

fn write_save_inner(path: &str, jobs: Vec<SaveJob>) -> Result<(), Box<dyn std::error::Error>> {
    let tmp = format!("{path}.tmp");
    if std::path::Path::new(&tmp).exists() {
        std::fs::remove_file(&tmp)?;
    }

    // Open, migrate, run all jobs in one transaction, then DROP the connection
    // before the atomic rename (Windows file-handle semantics — see
    // `with_db_connection`'s doc comment).
    with_db_connection(&tmp, |conn| {
        db_migrations::migrations::runner().run(conn)?;
        let tx = conn.transaction()?;
        for job in jobs {
            job(&tx)?;
        }
        tx.commit()?;
        Ok(())
    })?;

    std::fs::rename(&tmp, path)?;
    Ok(())
}
