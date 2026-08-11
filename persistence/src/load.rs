use bevy::{
    ecs::world::CommandQueue,
    input::common_conditions::input_just_released,
    platform::collections::HashMap,
    prelude::*,
    tasks::IoTaskPool,
};

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;

use crossbeam_channel::{Receiver, Sender, unbounded};

use game_core::prelude::*;
use logging::prelude::*;
use states::{AdminMode, prelude::*};

use crate::{
    common::{db_migrations, with_db_connection},
};

pub struct MapLoadPlugin;
impl Plugin for MapLoadPlugin {
    fn build(&self, app: &mut App) {
        app
            .init_resource::<GameLoadRegistry>()
            .init_resource::<LoadProgress>()
            .init_resource::<GameMapList>()
            // build_entity_id_map is exclusive, runs first in LoadMapInfo.
            // spawn_stage_loaders runs .after() it for LoadMapInfo, and standalone
            // for the other stages.
            .add_systems(
                OnEnter(MapLoadingStage::LoadMapInfo),
                (
                    build_entity_id_map,
                    spawn_stage_loaders.after(build_entity_id_map),
                ),
            )
            .add_systems(
                OnEnter(MapLoadingStage::LoadResources),
                spawn_stage_loaders,
            )
            .add_systems(
                OnEnter(MapLoadingStage::SpawnMapElements),
                spawn_stage_loaders,
            )
            .add_systems(
                OnEnter(MapLoadingStage::SpawnEffectInstances),
                spawn_stage_loaders,
            )
            .add_systems(OnEnter(MapLoadingStage::Ready), on_map_load_ready)
            .add_systems(Update, (
                apply_load_queues.run_if(in_state(GameState::Loading)),
                advance_stage
                    .run_if(in_state(GameState::Loading))
                    .after(apply_load_queues),
                LoadGameSignal::emit.run_if(input_just_released(KeyCode::KeyA)),
            ))
            .add_observer(LoadGameSignal::on_trigger);
    }
}

// --- Load pipeline types -----------------------------------------------------

/// A loader is a plain fn, not a trait impl. It runs on an IO thread with its own
/// connection. It must stream ONE pass over its table(s) — no LIMIT/OFFSET, no
/// resumability — and push world mutations through the context.
pub type LoaderFn = fn(&mut LoadContext) -> rusqlite::Result<()>;

pub(crate) struct LoaderDescriptor {
    /// Primary table; used for `SELECT COUNT(*)` progress totals.
    pub table: &'static str,
    pub run: LoaderFn,
}

#[derive(Resource, Default)]
pub(crate) struct GameLoadRegistry {
    pub loaders: HashMap<MapLoadingStage, Vec<LoaderDescriptor>>,
}

/// Old-id -> new-Entity map. Built once per load on the main thread, shared into
/// loader tasks.
#[derive(Resource, Clone)]
pub struct EntityIdMap(pub Arc<HashMap<i64, Entity>>);

const CHUNK_ROWS: usize = 128;

pub struct LoadContext<'a> {
    pub conn: &'a rusqlite::Connection,
    entity_map: Arc<HashMap<i64, Entity>>,
    queue: CommandQueue,
    rows_since_flush: usize,
    sender: Sender<CommandQueue>,
    done_rows: Arc<AtomicUsize>,
    cancel: Arc<AtomicBool>,
}
impl LoadContext<'_> {
    /// Map a saved entity id to the pre-spawned Entity. None => log a dev warn in
    /// the loader and `continue` (same policy as today).
    pub fn entity(&self, old_id: i64) -> Option<Entity> {
        self.entity_map.get(&old_id).copied()
    }

    /// `insert(bundle)` on a mapped entity. Sugar over `push()`.
    pub fn insert(&mut self, entity: Entity, bundle: impl Bundle) {
        self.push(move |world: &mut World| {
            if let Ok(mut e) = world.get_entity_mut(entity) {
                e.insert(bundle);
            }
        });
    }

    /// Insert/replace a resource. Sugar over `push()`.
    pub fn insert_resource(&mut self, resource: impl Resource) {
        self.push(move |world: &mut World| {
            world.insert_resource(resource);
        });
    }

    /// Escape hatch: arbitrary deferred world mutation. Increments `done_rows`,
    /// pushes into the current `CommandQueue`, flushes to the channel every
    /// `CHUNK_ROWS` (128). No-ops (drops `f`) when cancelled.
    pub fn push(&mut self, f: impl FnOnce(&mut World) + Send + 'static) {
        if self.cancelled() {
            return;
        }
        self.queue.push(f);
        self.done_rows.fetch_add(1, Ordering::Relaxed);
        self.rows_since_flush += 1;
        if self.rows_since_flush >= CHUNK_ROWS {
            self.flush();
        }
    }

    /// Loaders MAY check this in long loops to early-return `Ok(())`. Not required.
    pub fn cancelled(&self) -> bool {
        self.cancel.load(Ordering::Relaxed)
    }

    /// Send the current (possibly partial) `CommandQueue` to the applier channel
    /// and start a fresh one. Called automatically every `CHUNK_ROWS` by `push`,
    /// and once more on `Drop` for the tail.
    fn flush(&mut self) {
        if self.queue.is_empty() {
            self.rows_since_flush = 0;
            return;
        }
        let queue = std::mem::take(&mut self.queue);
        let _ = self.sender.send(queue);
        self.rows_since_flush = 0;
    }
}
impl Drop for LoadContext<'_> {
    fn drop(&mut self) {
        // Flush the remaining partial queue. Don't lose the tail.
        self.flush();
    }
}

/// Per-load runtime state. Created on `LoadGameSignal`, dropped on reaching
/// `Ready`. MUST exist before `Init`'s first `Update` frame — `advance_stage`
/// runs on that frame (zero loaders) and reads `LoadRunner.tasks` /
/// `LoadRunner.receiver`.
#[derive(Resource)]
pub(crate) struct LoadRunner {
    /// Keep handles: dropping a `Task` cancels it.
    pub tasks: Vec<bevy::tasks::Task<()>>,
    pub sender: Sender<CommandQueue>,
    pub receiver: Receiver<CommandQueue>,
    pub cancel: Arc<AtomicBool>,
}

/// Public — HUD reads this for a progress bar.
#[derive(Resource, Default)]
pub struct LoadProgress {
    pub total_rows: usize,
    pub(crate) done_rows: Arc<AtomicUsize>,
}
impl LoadProgress {
    pub fn done_rows(&self) -> usize {
        self.done_rows.load(Ordering::Relaxed)
    }
    pub fn fraction(&self) -> f32 {
        if self.total_rows == 0 {
            1.0
        } else {
            self.done_rows() as f32 / self.total_rows as f32
        }
    }
}

/// Command queued by the `LoadGameSignal` observer to set up the load runner.
/// Runs at the observer's sync point — before state transitions and before
/// `advance_stage`'s first `Update` run.
struct InitLoadRunner;
impl Command for InitLoadRunner {
    type Out = ();
    fn apply(self, world: &mut World) {
        // Cancel-and-replace: if a previous LoadRunner exists (e.g. re-load
        // during an in-flight load), flag it cancelled before replacing.
        if let Some(existing) = world.get_resource::<LoadRunner>() {
            existing.cancel.store(true, Ordering::Relaxed);
        }
        let (sender, receiver) = unbounded();
        world.insert_resource(LoadRunner {
            tasks: Vec::new(),
            sender,
            receiver,
            cancel: Arc::new(AtomicBool::new(false)),
        });
        // Fresh done_rows Arc per load — never reuse the previous load's counter.
        world.insert_resource(LoadProgress::default());
    }
}

// --- Runner systems ----------------------------------------------------------

/// `OnEnter(MapLoadingStage::LoadMapInfo)`. **Exclusive system** — uses
/// `world.spawn_empty()` / `world.insert_resource(...)` directly, NOT `Commands`:
/// deferred `Commands` inserts land at the `OnEnter` schedule's end sync point
/// and would be invisible to `spawn_stage_loaders` running `.after()` it in the
/// same schedule.
pub(crate) fn build_entity_id_map(world: &mut World) {
    let config = world.resource::<LoadMapConfig>().clone();

    let map_path = match &config.source {
        // New map: no DB, no entities, no rows.
        MapSource::New(_) => {
            world.insert_resource(EntityIdMap(Arc::new(HashMap::new())));
            world.resource_mut::<LoadProgress>().total_rows = 0;
            Log::debug()
                .dev()
                .tag(Tag::GameLoad)
                .message("EntityIdMap population skipped");
            return;
        }
        MapSource::File(map_path) => map_path.clone(),
    };
    let registry = world.resource::<GameLoadRegistry>();

    // Compute progress totals by summing COUNT(*) over every registered table
    // (all stages).
    let mut total_rows: usize = 0;
    let _ = with_db_connection(&map_path, |conn| {
        for loaders in registry.loaders.values() {
            for desc in loaders {
                let count: i64 = conn
                    .query_row(&format!("SELECT COUNT(*) FROM {}", desc.table), [], |row| {
                        row.get(0)
                    })
                    .unwrap_or(0);
                total_rows += count as usize;
            }
        }
        Ok(())
    });

    // Build the entity id map: read `entities` table, spawn an empty entity per
    // row, map old_id -> new Entity.
    let mut map: HashMap<i64, Entity> = HashMap::new();
    let _ = with_db_connection(&map_path, |conn| {
        let mut stmt = conn.prepare("SELECT id FROM entities")?;
        let rows = stmt.query_map([], |row| row.get::<_, i64>(0))?;
        for row in rows {
            let old_id = row?;
            let new_entity = world.spawn_empty().id();
            map.insert(old_id, new_entity);
        }
        Ok(())
    });

    let count = map.len();
    Log::debug()
        .dev()
        .tag(Tag::GameLoad)
        .message(format!("EntityIdMap populated: {count} entities; total_rows={total_rows}"));

    let entity_id_map = Arc::new(map);
    world.insert_resource(EntityIdMap(entity_id_map));

    // Set progress totals.
    let mut progress = world.resource_mut::<LoadProgress>();
    progress.total_rows = total_rows;
}

/// `OnEnter(...)` for each load stage. For each descriptor of the entered stage:
/// spawn an `IoTaskPool` task that opens its own connection, builds a
/// `LoadContext`, calls `run`, logs `Err` with the table name, flushes the tail.
pub(crate) fn spawn_stage_loaders(
    stage: Res<State<MapLoadingStage>>,
    mut runner: ResMut<LoadRunner>,
    registry: Res<GameLoadRegistry>,
    entity_id_map: Res<EntityIdMap>,
    load_config: Res<LoadMapConfig>,
    progress: Res<LoadProgress>,
) {
    let map_path = match &load_config.source {
        // New map: no loaders to spawn. The stage machine advances on its own
        // (zero tasks ⇒ one stage per frame).
        MapSource::New(_) => return,
        MapSource::File(map_path) => map_path.clone(),
    };

    let target_stage = stage.get();
    let Some(descriptors) = registry.loaders.get(target_stage) else {
        return;
    };
    if descriptors.is_empty() {
        return;
    }

    let entity_map = entity_id_map.0.clone();
    let sender = runner.sender.clone();
    let done_rows = progress.done_rows.clone();
    let cancel = runner.cancel.clone();

    Log::debug()
        .dev()
        .tag(Tag::GameLoad)
        .message(format!("Spawning {} loader(s) for {target_stage:?}", descriptors.len()));

    for desc in descriptors {
        let table = desc.table;
        let run = desc.run;
        let map_path = map_path.clone();
        let entity_map = entity_map.clone();
        let sender = sender.clone();
        let done_rows = done_rows.clone();
        let cancel = cancel.clone();

        let task = IoTaskPool::get().spawn(async move {
            let result = with_db_connection(&map_path, |conn| {
                let mut ctx = LoadContext {
                    conn,
                    entity_map,
                    queue: CommandQueue::default(),
                    rows_since_flush: 0,
                    sender,
                    done_rows,
                    cancel,
                };
                run(&mut ctx)?;
                // Drop flushes the tail queue.
                Ok(())
            });
            if let Err(e) = result {
                Log::error()
                    .dev()
                    .tag(Tag::GameLoad)
                    .message(format!("Loader for '{table}' failed: {e}"));
            }
        });
        runner.tasks.push(task);
    }
}

/// `Update`, `run_if(in_state(GameState::Loading))`. Drains the channel within a
/// time budget and merges `CommandQueue`s into the frame via
/// `commands.append(&mut queue)`. Disk I/O never sits inside the frame; the
/// budget only bounds ECS application.
pub(crate) fn apply_load_queues(mut commands: Commands, runner: Res<LoadRunner>) {
    let start = std::time::Instant::now();
    let budget = std::time::Duration::from_millis(4);
    while start.elapsed() < budget {
        match runner.receiver.try_recv() {
            Ok(mut queue) => {
                commands.append(&mut queue);
            }
            Err(_) => break, // channel empty
        }
    }
}

/// `Update`, `.after(apply_load_queues)`. Advances to `stage.next()` only when:
/// all loader tasks are finished and the channel is drained.
pub(crate) fn advance_stage(
    mut runner: ResMut<LoadRunner>,
    stage: Res<State<MapLoadingStage>>,
    mut next_stage: ResMut<NextState<MapLoadingStage>>,
) {
    // Drop finished tasks (retain unfinished ones — dropping cancels).
    runner.tasks.retain(|t| !t.is_finished());

    if !runner.tasks.is_empty() {
        return;
    }
    if !runner.receiver.is_empty() {
        return;
    }

    let Some(next) = stage.get().next() else {
        return;
    };
    Log::debug()
        .dev()
        .tag(Tag::GameLoad)
        .message(format!("Stage complete, advancing to {next:?}"));
    next_stage.set(next);
}

/// Public accessor for the `LoadGameSignal` observer to queue the runner setup
/// command.
pub(crate) fn queue_init_load_runner(commands: &mut Commands) {
    commands.queue(InitLoadRunner);
}

// --- Shared infrastructure ---------------------------------------------------

/// All .dwd map files found in the maps/ directory at startup.
#[derive(Resource)]
pub struct GameMapList {
    pub names: Vec<String>,
}
impl Default for GameMapList {
    fn default() -> Self {
        let mut list = Self { names: vec![] };
        list.refresh();
        list
    }
}
impl GameMapList {
    pub fn paths(&self) -> Vec<String> {
        self.names.iter().map(|name| format!("maps/{}.dwd", name)).collect()
    }

    /// Re-scans the `maps/` directory. Called by the finalize system after a
    /// scenario save so the new map appears in the menu without restarting.
    pub fn refresh(&mut self) {
        self.names = std::fs::read_dir("maps")
            .ok()
            .into_iter()
            .flat_map(|rd| rd.filter_map(|e| e.ok()))
            .filter_map(|e| {
                let path = e.path();
                if path.is_file() && path.extension().and_then(|s| s.to_str()) == Some("dwd") {
                    path.file_stem().and_then(|s| s.to_str()).map(|s| s.to_string())
                } else {
                    None
                }
            })
            .collect();
    }
}

/// Where the map being loaded comes from.
///
/// `File` loads an existing `.dwd` file.
/// `New` builds a blank map from `MapInfo` without reading or creating a file.
#[derive(Clone)]
pub enum MapSource {
    File(String),
    New(MapInfo),
}

#[derive(Resource, Clone)]
pub struct LoadMapConfig {
    pub source: MapSource,
    pub game_start_state: GameState,
    pub admin_mode: AdminMode,
}
impl LoadMapConfig {
    /// Load an existing `.dwd` file. Running + admin disabled — the normal play path.
    pub fn file(map_path: impl Into<String>) -> Self {
        Self {
            source: MapSource::File(map_path.into()),
            game_start_state: GameState::Running,
            admin_mode: AdminMode::Disabled,
        }
    }

    /// Build a blank map in memory. Paused + admin enabled — ready to author.
    pub fn new_map(map_info: MapInfo) -> Self {
        Self {
            source: MapSource::New(map_info),
            game_start_state: GameState::Paused,
            admin_mode: AdminMode::Enabled,
        }
    }
}

/// True while the in-flight map build is a fresh map rather than a file load.
///
/// Only valid inside the map build window — `OnEnter(Init)` through
/// `OnEnter(Ready)` inclusive. `LoadMapConfig` does not exist outside it, so
/// registering this condition outside that window panics when it runs.
pub fn creating_new_map(config: Res<LoadMapConfig>) -> bool {
    matches!(config.source, MapSource::New(_))
}

#[derive(Event)]
pub struct LoadGameSignal(pub LoadMapConfig);
impl LoadGameSignal {
    fn emit(mut commands: Commands) {
        Log::debug().dev().tag(Tag::GameLoad).message("Triggering load signal");
        commands.trigger(LoadGameSignal(LoadMapConfig::file("test_save.dwd")));
    }
    fn on_trigger(
        trigger: On<LoadGameSignal>,
        mut commands: Commands,
        mut next_game_state: ResMut<NextState<GameState>>,
        mut next_map_loading_stage: ResMut<NextState<MapLoadingStage>>,
        mut next_ui_state: ResMut<NextState<UiInteraction>>,
        map_bound_entities: Query<Entity, With<MapBound>>,
    ) {
        let config = trigger.event().0.clone();

        match &config.source {
            MapSource::File(map_path) => {
                Log::info().dev().tag(Tag::GameLoad).message(format!("Loading '{map_path}'"));
                // Run migrations synchronously on main thread before parallel loading starts.
                // Skipped for New: rusqlite::Connection::open *creates* the file, which would
                // litter maps/<name>.dwd on disk before the user has saved anything.
                with_db_connection(map_path, |conn| {
                    //conn.execute("DELETE FROM refinery_schema_history;", [])?; // Used to clear refinery migrations history, uncomment when in need.
                    db_migrations::migrations::runner().run(conn)?;
                    Ok(())
                }).expect("Failed to run migrations on load");
            }
            MapSource::New(map_info) => {
                Log::info().dev().tag(Tag::GameLoad).message(format!("Creating new map '{}'", map_info.name));
            }
        }

        // Set up the load runner (channel, LoadRunner, fresh LoadProgress)
        // before state transitions. Runs at the observer's sync point — before
        // advance_stage's first Update run on the Init stage.
        queue_init_load_runner(&mut commands);

        commands.insert_resource(config);
        next_game_state.set(GameState::Loading);
        next_map_loading_stage.set(MapLoadingStage::Init);
        next_ui_state.set(UiInteraction::Free);

        // Despawn all existing map elements
        map_bound_entities.iter().for_each(|entity| commands.entity(entity).despawn());
    }
}

fn on_map_load_ready(
    mut commands: Commands,
    load_config: Res<LoadMapConfig>,
    mut next_admin_mode: ResMut<NextState<AdminMode>>,
    mut next_game_state: ResMut<NextState<GameState>>,
) {
    Log::info().player().tag(Tag::GameLoad).message("Game loaded");
    next_game_state.set(load_config.game_start_state);
    (*next_admin_mode).set_if_neq(load_config.admin_mode);
    commands.remove_resource::<LoadMapConfig>();
}
