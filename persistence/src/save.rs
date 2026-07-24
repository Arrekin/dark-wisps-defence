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
use crate::load::GameMapList;

pub struct MapSavePlugin;
impl Plugin for MapSavePlugin {
    fn build(&self, app: &mut App) {
        app
            // Schedule must EXIST even with zero collectors (T1 has none), or
            // `world.run_schedule(CollectSave)` panics.
            .init_schedule(CollectSave)
            .init_resource::<PendingSaveJobs>()
            .add_systems(Update, SaveGameSignal::emit_quick.run_if(input_just_released(KeyCode::KeyZ)))
            .add_systems(Update, finalize_save.run_if(resource_exists::<SaveContext>))
            .add_systems(Last, drive_save.run_if(resource_added::<SaveContext>))
            .add_observer(on_save_game_signal)
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

// ============================================================================
// SAVE SIGNAL + CONTEXT
// ============================================================================

#[derive(Debug, Clone)]
pub enum SaveTarget {
    /// `test_save.dwd` (dev keybind; future: named slots as File(path))
    Quick,
    /// `maps/<name>.dwd` + scenario mode (reset playthrough metadata on write)
    Scenario(String),
}

#[derive(Event, Debug, Clone)]
pub struct SaveGameSignal {
    pub target: SaveTarget,
}

impl SaveGameSignal {
    /// Z always saves to `test_save.dwd`, even after loading `maps/<name>.dwd`.
    fn emit_quick(mut commands: Commands) {
        commands.trigger(SaveGameSignal { target: SaveTarget::Quick });
    }
}

/// The save lifecycle: guard + mode carrier + completion signal in one.
/// Inserted by the repack observer, removed by the finalize system.
#[derive(Resource)]
pub struct SaveContext {
    pub path: String,
    pub save_as_scenario: bool, // Whether to reset metadata, for example whether game_start was emitted.
    pub done: Arc<AtomicBool>,
    pub error: Arc<AtomicBool>,
}

/// One global observer: `SaveContext` exists → log + exit (in-flight block).
/// Else repack: resolve path from target, insert `SaveContext` — the one place
/// requests become plans.
fn on_save_game_signal(
    trigger: On<SaveGameSignal>,
    mut commands: Commands,
    save_ctx: Option<Res<SaveContext>>,
) {
    if save_ctx.is_some() {
        Log::warn()
            .player()
            .tag(Tag::GameSave)
            .message("Save already in flight — skipping");
        return;
    }
    let target = trigger.event().target.clone();
    let (path, save_as_scenario) = match target {
        SaveTarget::Quick => ("test_save.dwd".to_string(), false),
        SaveTarget::Scenario(name) => (format!("maps/{}.dwd", name), true),
    };
    commands.insert_resource(SaveContext {
        path,
        save_as_scenario,
        done: Arc::new(AtomicBool::new(false)),
        error: Arc::new(AtomicBool::new(false)),
    });
}

// ============================================================================
// DRIVER
// ============================================================================

/// Exclusive save driver. Runs in `Last` only when `SaveContext` was added
/// this frame. Collects jobs from the `CollectSave` schedule, hands them to
/// one detached IO task that writes `<path>.tmp` and atomically renames.
fn drive_save(world: &mut World) {
    // 1. Run the collector schedule (collectors read SaveContext for scenario mode).
    world.run_schedule(CollectSave);

    // 2. Take the job buffer. The emptiness check is on this taken vec, AFTER
    //    run_schedule — never a pre-check before it.
    let jobs = std::mem::take(&mut world.resource_mut::<PendingSaveJobs>().0);
    if jobs.is_empty() {
        Log::warn()
            .dev()
            .tag(Tag::GameSave)
            .message("SaveGameSignal fired but no jobs were collected — nothing to write");
        // Remove the context — nothing to wait for.
        world.remove_resource::<SaveContext>();
        return;
    }

    // 3. Hand off to a detached IO task.
    let save_ctx = world.resource::<SaveContext>();
    let path = save_ctx.path.clone();
    let done = save_ctx.done.clone();
    let error = save_ctx.error.clone();
    Log::info()
        .dev()
        .tag(Tag::GameSave)
        .message(format!("Saving game to '{path}' ({} jobs)", jobs.len()));

    IoTaskPool::get()
        .spawn(async move {
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
                    error.store(true, Ordering::Relaxed);
                }
            }
            done.store(true, Ordering::Relaxed);
        })
        .detach();
}

fn write_save_inner(path: &str, jobs: Vec<SaveJob>) -> Result<(), Box<dyn std::error::Error>> {
    let tmp = format!("{path}.tmp");
    if std::path::Path::new(&tmp).exists() {
        std::fs::remove_file(&tmp)?;
    }

    // Open, migrate, run all jobs in one transaction, then DROP the connection
    // before the atomic rename (Windows file-handle semantics — see
    // `with_db_connection`'s doc comment).
    if let Err(e) = with_db_connection(&tmp, |conn| {
        db_migrations::migrations::runner().run(conn)?;
        let tx = conn.transaction()?;
        for job in jobs {
            job(&tx)?;
        }
        tx.commit()?;
        Ok(())
    }) {
        let _ = std::fs::remove_file(&tmp);
        return Err(e);
    }

    std::fs::rename(&tmp, path)?;
    Ok(())
}

// ============================================================================
// FINALIZE
// ============================================================================

/// Polls `SaveContext.done` every frame. On completion (success or error),
/// removes `SaveContext` (reopening the guard). On scenario save, rescans
/// `GameMapList` so the new map appears in the menu without restarting.
fn finalize_save(
    mut commands: Commands,
    save_ctx: Res<SaveContext>,
    mut map_list: ResMut<GameMapList>,
) {
    if !save_ctx.done.load(Ordering::Relaxed) { return; }
    if save_ctx.error.load(Ordering::Relaxed) {
        Log::warn()
            .player()
            .tag(Tag::GameSave)
            .message("Save failed — context cleared");
    }
    if save_ctx.save_as_scenario {
        map_list.refresh();
    }
    commands.remove_resource::<SaveContext>();
}
