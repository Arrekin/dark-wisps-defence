//! # Map loading
//!
//! A file load runs migrations, replaces map-bound entities, and enters `GameState::Loading`.
//! Requests during a load or a pending state transition are rejected. New maps use the same
//! stages without reading a file.
//!
//! Loaders run on IO threads and send batches of world changes to the main thread. Each
//! `MapLoadingStage` waits for its loaders and their queued changes before advancing, so later
//! stages can use entities and resources created earlier. The game drains batches over multiple
//! frames instead of waiting for all IO to finish before processing them.
//!
//! At `Ready`, the game queues the requested start state. It sends a `LoadGameReport` when it
//! leaves `Loading`, after that state takes effect.

use std::{
    str::FromStr,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
};

use bevy::{
    ecs::{system::command, world::CommandQueue},
    input::common_conditions::input_just_released,
    platform::collections::HashMap,
    prelude::*,
    tasks::IoTaskPool,
};
use crossbeam_channel::{Receiver, Sender, unbounded};
use serde::Serialize;

use game_core::prelude::*;
use logging::prelude::*;
use states::{AdminMode, prelude::*};

use crate::{
    common::{Migrations, with_db_connection},
    map_file_name::MapFileName,
    map_list::GameMapList,
};

pub struct MapLoadPlugin;
impl Plugin for MapLoadPlugin {
    fn build(&self, app: &mut App) {
        app
            .init_resource::<GameLoadRegistry>()
            .init_resource::<LoadProgress>()
            .init_resource::<GameMapList>()
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
            .add_systems(OnExit(GameState::Loading), finish_map_load)
            .add_systems(Update, (
                apply_load_queues.run_if(in_state(GameState::Loading)),
                advance_stage
                    .run_if(in_state(GameState::Loading))
                    .after(apply_load_queues),
                LoadGameSignal::emit_load_quicksave.run_if(input_just_released(KeyCode::KeyA)),
            ))
            .add_observer(LoadGameSignal::on_load_game_signal_do_so);
    }
}

// --- Loaders -----------------------------------------------------------------

/// Read each table in one pass and send world changes through `LoadContext`. The context batches
/// those changes across frames; loaders should not paginate their queries.
pub type LoaderFn = fn(&mut LoadContext) -> LoadResult;

pub type LoadResult<T = ()> = Result<T, LoadError>;

/// Why a loader or one saved entity could not be loaded. [`LoadContext::for_each_entity`] logs
/// an entity's error and moves on to the next entity; a loader's own error ends that loader.
#[derive(Debug)]
pub enum LoadError {
    Database(rusqlite::Error),
    /// The saved ID has no entity allocated in [`EntityIdMap`].
    UnmappedEntity(u32),
    /// Reading a named table failed, including finding no row where one is required.
    TableReadFailed { table: &'static str, error: rusqlite::Error },
    /// A required row of a child table is absent.
    MissingRow { table: &'static str },
    /// A stored name matches no variant the game knows.
    UnknownValue { kind: &'static str, value: String },
}
impl LoadError {
    pub fn unknown_value(kind: &'static str, value: impl Into<String>) -> Self {
        Self::UnknownValue { kind, value: value.into() }
    }

    /// Error conversion naming the table a query read: `.map_err(LoadError::table_read("grid_coords"))`.
    pub fn table_read(table: &'static str) -> impl FnOnce(rusqlite::Error) -> Self {
        move |error| Self::TableReadFailed { table, error }
    }
}
impl From<rusqlite::Error> for LoadError {
    fn from(error: rusqlite::Error) -> Self {
        Self::Database(error)
    }
}
impl std::fmt::Display for LoadError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Database(error) => write!(formatter, "database error: {error}"),
            Self::UnmappedEntity(old_id) => write!(formatter, "saved ID {old_id} has no allocated entity"),
            Self::TableReadFailed { table, error } => write!(formatter, "reading '{table}' failed: {error}"),
            Self::MissingRow { table } => write!(formatter, "no row in '{table}'"),
            Self::UnknownValue { kind, value } => write!(formatter, "unknown {kind} '{value}'"),
        }
    }
}
impl std::error::Error for LoadError {}

pub trait LoadRowExtension {
    /// Reads a column holding a stored name and parses it into `T`; a name `T` does not know
    /// becomes [`LoadError::UnknownValue`] labelled with `T`'s type name.
    fn get_parsed<T: FromStr>(&self, index: usize) -> LoadResult<T>;
}
impl LoadRowExtension for rusqlite::Row<'_> {
    fn get_parsed<T: FromStr>(&self, index: usize) -> LoadResult<T> {
        let name: String = self.get(index)?;
        name.parse().map_err(|_| {
            let kind = std::any::type_name::<T>().rsplit("::").next().unwrap_or_default();
            LoadError::unknown_value(kind, name)
        })
    }
}

pub(crate) struct LoaderDescriptor {
    /// Table counted for the progress bar; loaders can read additional tables.
    pub table: &'static str,
    pub run: LoaderFn,
}

#[derive(Resource, Default)]
pub(crate) struct GameLoadRegistry {
    pub loaders: HashMap<MapLoadingStage, Vec<LoaderDescriptor>>,
}

/// Maps saved IDs to entities allocated before loading begins, including entities populated in
/// later stages.
#[derive(Resource, Clone)]
pub struct EntityIdMap(pub Arc<HashMap<u32, Entity>>);

const CHUNK_ROWS: usize = 128;

/// Queues loader changes for the main thread; flushes every `CHUNK_ROWS` changes and on drop.
pub struct LoadContext<'a> {
    pub conn: &'a rusqlite::Connection,
    /// The table the loader is registered for; names the loader in logs.
    table: &'static str,
    entity_map: Arc<HashMap<u32, Entity>>,
    queue: CommandQueue,
    rows_since_flush: usize,
    sender: Sender<CommandQueue>,
    done_rows: Arc<AtomicUsize>,
}
/// Database side: walks the save file's tables and reads the shared ones.
impl LoadContext<'_> {
    /// Runs `load_row` for every row of `query`. A row that fails to load is logged and skipped;
    /// the rest of the table still loads. Only a failure to prepare the query or to step to the
    /// next row ends the table early.
    #[log_tags(Tag::GameLoad)]
    pub fn for_each_row(
        &mut self,
        query: &str,
        mut load_row: impl FnMut(&mut Self, &rusqlite::Row) -> LoadResult,
    ) -> LoadResult {
        let conn = self.conn;
        let mut statement = conn.prepare(query)?;
        let mut rows = statement.query([])?;
        while let Some(row) = rows.next()? {
            let _ = load_row(self, row)
                .inspect_err(|error| error_dev!("'{}' row skipped: {error}", self.table));
        }
        Ok(())
    }

    /// [`Self::for_each_row`] for tables whose first column is the saved entity ID.
    /// `load_entity` receives that ID and the entity it maps to; the skip log names the ID.
    #[log_tags(Tag::GameLoad)]
    pub fn for_each_entity(
        &mut self,
        query: &str,
        mut load_entity: impl FnMut(&mut Self, u32, Entity, &rusqlite::Row) -> LoadResult,
    ) -> LoadResult {
        self.for_each_row(query, |ctx, row| {
            let old_id: u32 = row.get(0)?;
            let _ = ctx.entity(old_id)
                .and_then(|entity| load_entity(ctx, old_id, entity, row))
                .inspect_err(|error| error_dev!("'{}' entity {old_id} skipped: {error}", ctx.table));
            Ok(())
        })
    }

    pub fn world_position(&self, old_id: u32) -> LoadResult<Vec2> {
        self.query_row_cached("world_positions", "SELECT x, y FROM world_positions WHERE entity_id = ?1", [old_id], |row| Ok(Vec2::new(row.get(0)?, row.get(1)?)))
    }

    pub fn integrity_points(&self, old_id: u32) -> LoadResult<f32> {
        self.query_row_cached("integrity_points", "SELECT current FROM integrity_points WHERE entity_id = ?1", [old_id], |row| row.get(0))
    }

    pub fn disabled_by_player(&self, old_id: u32) -> LoadResult<bool> {
        Ok(self.conn.prepare_cached("SELECT 1 FROM disabled_by_player WHERE entity_id = ?1")?.exists([old_id])?)
    }

    pub fn stat(&self, stat_name: &str) -> LoadResult<f32> {
        self.query_row_cached("stats", "SELECT stat_value FROM stats WHERE stat_name = ?1", [stat_name], |row| row.get(0))
    }

    pub fn grid_coords(&self, old_id: u32) -> LoadResult<GridCoords> {
        self.query_row_cached("grid_coords", "SELECT x, y FROM grid_coords WHERE entity_id = ?1", [old_id], |row| Ok(GridCoords { x: row.get(0)?, y: row.get(1)? }))
    }

    pub fn grid_imprint(&self, old_id: u32) -> LoadResult<GridImprint> {
        let (shape, width, height): (String, i32, Option<i32>) = self.query_row_cached(
            "grid_imprints",
            "SELECT shape, width, height FROM grid_imprints WHERE id = ?1",
            [old_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )?;
        match (shape.as_str(), height) {
            ("Rectangle", Some(height)) => Ok(GridImprint::Rectangle { width, height }),
            ("Plus", _) => Ok(GridImprint::Plus { extents: width }),
            _ => Err(LoadError::unknown_value("grid imprint", format!("{shape} width={width} height={height:?}"))),
        }
    }

    /// Reads the one row `query` selects through the connection's statement cache, so
    /// per-entity reads skip re-preparing their SQL. Any failure, including a missing row,
    /// becomes [`LoadError::TableReadFailed`] naming `table`.
    fn query_row_cached<T>(
        &self,
        table: &'static str,
        query: &str,
        params: impl rusqlite::Params,
        map_row: impl FnOnce(&rusqlite::Row) -> rusqlite::Result<T>,
    ) -> LoadResult<T> {
        self.conn.prepare_cached(query)
            .and_then(|mut statement| statement.query_row(params, map_row))
            .map_err(LoadError::table_read(table))
    }
}

/// Game side: maps saved IDs to entities and queues world changes for the main thread.
impl LoadContext<'_> {
    /// Resolves a saved ID to the entity allocated for it before loading began.
    pub fn entity(&self, old_id: u32) -> LoadResult<Entity> {
        self.entity_map.get(&old_id).copied().ok_or(LoadError::UnmappedEntity(old_id))
    }

    /// Resolves an optional saved reference: no ID is `Ok(None)`, an unmapped ID is an error.
    pub fn optional_entity(&self, old_id: Option<u32>) -> LoadResult<Option<Entity>> {
        old_id.map(|old_id| self.entity(old_id)).transpose()
    }

    /// Inserts only if the entity still exists when the batch is applied.
    pub fn insert(&mut self, entity: Entity, bundle: impl Bundle) {
        self.push(move |world: &mut World| {
            if let Ok(mut entity_mut) = world.get_entity_mut(entity) {
                entity_mut.insert(bundle);
            }
        });
    }

    pub fn insert_resource(&mut self, resource: impl Resource) {
        self.push(command::insert_resource(resource));
    }

    /// Triggers `event` when the batch is applied.
    pub fn trigger<'a>(&mut self, event: impl Event<Trigger<'a>: Default>) {
        self.push(command::trigger(event));
    }

    /// Queues a world change and counts it toward loading progress.
    pub fn push(&mut self, change: impl Command) {
        self.queue.push(change.handle_error());
        self.done_rows.fetch_add(1, Ordering::Relaxed);
        self.rows_since_flush += 1;
        if self.rows_since_flush >= CHUNK_ROWS {
            self.flush();
        }
    }

    #[log_tags(Tag::GameLoad)]
    fn flush(&mut self) {
        if self.queue.is_empty() {
            self.rows_since_flush = 0;
            return;
        }
        let queue = std::mem::take(&mut self.queue);
        let _ = self.sender.send(queue)
            .inspect_err(|_| error_dev!("Load batch dropped: the load runner is gone"));
        self.rows_since_flush = 0;
    }
}
impl Drop for LoadContext<'_> {
    fn drop(&mut self) {
        self.flush();
    }
}

// --- Load in progress --------------------------------------------------------

/// Holds loader tasks and their batch channel until the game leaves `Loading`.
/// It must exist before the first `Update`: the empty `Init` stage advances immediately.
#[derive(Resource)]
pub(crate) struct LoadRunner {
    /// Kept until finished: dropping a `Task` cancels it.
    pub tasks: Vec<bevy::tasks::Task<()>>,
    pub sender: Sender<CommandQueue>,
    pub receiver: Receiver<CommandQueue>,
}

/// Progress is approximate: `done_rows` counts queued changes, while `total_rows` counts rows
/// in registered tables. The counts need not match, and queued changes may not yet be applied.
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

/// Installs a fresh runner and progress counter before the load's first `Update`.
struct InitLoadRunner;
impl Command for InitLoadRunner {
    type Out = ();
    fn apply(self, world: &mut World) {
        let (sender, receiver) = unbounded();
        world.insert_resource(LoadRunner {
            tasks: Vec::new(),
            sender,
            receiver,
        });
        world.insert_resource(LoadProgress::default());
    }
}

/// Allocates entities for saved IDs and counts loader rows before the first loader starts.
/// Runs exclusively because the loaders in this `OnEnter` schedule need the ID map immediately;
/// deferred commands would apply too late.
#[log_tags(Tag::GameLoad)]
pub(crate) fn build_entity_id_map(world: &mut World) {
    let config = world.resource::<LoadMapConfig>().clone();

    let map_path = match &config.source {
        #[debug_dev("EntityIdMap population skipped because map is new")]
        MapSource::New(_) => {
            world.insert_resource(EntityIdMap(Arc::new(HashMap::new())));
            world.resource_mut::<LoadProgress>().total_rows = 0;
            return;
        }
        MapSource::File(map_path) => map_path.clone(),
    };
    let mut total_rows: usize = 0;
    let mut map: HashMap<u32, Entity> = HashMap::new();
    #[debug_dev("EntityIdMap populated: {} entities; total_rows={total_rows}", map.len())]
    with_db_connection(&map_path, Migrations::Skip, |conn| {
        for loaders in world.resource::<GameLoadRegistry>().loaders.values() {
            for descriptor in loaders {
                let table = descriptor.table;
                let count: usize = conn
                    .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| row.get(0))
                    .inspect_err(|error| warn_dev!("Row count of '{table}' not read ({error}); progress counts it as 0"))
                    .unwrap_or(0);
                total_rows += count;
            }
        }

        let mut stmt = conn.prepare("SELECT id FROM entities")?;
        let rows = stmt.query_map([], |row| row.get::<_, u32>(0))?;
        for row in rows {
            let old_id = row?;
            let new_entity = world.spawn_empty().id();
            map.insert(old_id, new_entity);
        }
        Ok(())
    })
    .expect("Failed to read entity IDs from the map file");

    let entity_id_map = Arc::new(map);
    world.insert_resource(EntityIdMap(entity_id_map));

    let mut progress = world.resource_mut::<LoadProgress>();
    progress.total_rows = total_rows;
}

/// Starts one IO task and database connection per loader in this stage. Loader errors are
/// logged, but do not stop the load.
#[log_tags(Tag::GameLoad)]
pub(crate) fn spawn_stage_loaders(
    stage: Res<State<MapLoadingStage>>,
    mut runner: ResMut<LoadRunner>,
    registry: Res<GameLoadRegistry>,
    entity_id_map: Res<EntityIdMap>,
    load_config: Res<LoadMapConfig>,
    progress: Res<LoadProgress>,
) {
    let map_path = match &load_config.source {
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

    debug_dev!("Spawning {} loader(s) for {target_stage:?}", descriptors.len());

    for descriptor in descriptors {
        let table = descriptor.table;
        let run = descriptor.run;
        let map_path = map_path.clone();
        let entity_map = entity_map.clone();
        let sender = sender.clone();
        let done_rows = done_rows.clone();

        let task = IoTaskPool::get().spawn(async move {
            let _ = with_db_connection(&map_path, Migrations::Skip, |conn| {
                let mut ctx = LoadContext {
                    conn,
                    table,
                    entity_map,
                    queue: CommandQueue::default(),
                    rows_since_flush: 0,
                    sender,
                    done_rows,
                };
                run(&mut ctx)?;
                Ok(())
            })
            .inspect_err(|error| error_dev!("Loader for '{table}' failed: {error}"));
        });
        runner.tasks.push(task);
    }
}

/// Drains loader batches for up to 4 ms, appending them to this frame's commands. The budget
/// limits time spent draining, not the later cost of applying those commands.
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

/// Advances only after all tasks finish and the batch channel is empty. Runs after
/// `apply_load_queues`, whose commands are applied before the next stage starts.
#[log_tags(Tag::GameLoad)]
pub(crate) fn advance_stage(
    mut runner: ResMut<LoadRunner>,
    stage: Res<State<MapLoadingStage>>,
    mut next_stage: ResMut<NextState<MapLoadingStage>>,
) {
    runner.tasks.retain(|task| !task.is_finished());

    if !runner.tasks.is_empty() {
        return;
    }
    if !runner.receiver.is_empty() {
        return;
    }

    let Some(next) = stage.get().next() else {
        return;
    };
    debug_dev!("Stage complete, advancing to {next:?}");
    next_stage.set(next);
}

// --- Load request ------------------------------------------------------------

/// An existing `.dwd` file or a blank map built from `MapInfo` without creating a file.
#[derive(Clone)]
pub enum MapSource {
    File(String),
    New(MapInfo),
}

/// Load settings kept as a resource until the game leaves `Loading`.
#[derive(Resource, Clone)]
pub struct LoadMapConfig {
    pub source: MapSource,
    pub game_start_state: GameState,
    pub admin_mode: AdminMode,
    /// Recipient of the `LoadGameReport`.
    pub response: ResponseRequest,
}
impl LoadMapConfig {
    /// Load a file into normal play (running, admin disabled).
    pub fn file(map_path: impl Into<String>) -> Self {
        Self {
            source: MapSource::File(map_path.into()),
            game_start_state: GameState::Running,
            admin_mode: AdminMode::Disabled,
            response: ResponseRequest::not_needed(),
        }
    }

    /// Load a file from `maps/` using the normal play settings.
    pub fn map(file_name: &MapFileName) -> Self {
        Self::file(file_name.path())
    }

    /// Build a blank map in memory. Paused + admin enabled — ready to author.
    pub fn new_map(map_info: MapInfo) -> Self {
        Self {
            source: MapSource::New(map_info),
            game_start_state: GameState::Paused,
            admin_mode: AdminMode::Enabled,
            response: ResponseRequest::not_needed(),
        }
    }

    pub fn with_response(mut self, response: ResponseRequest) -> Self {
        self.response = response;
        self
    }
}

/// Run condition for systems used only while building a new map. Requires `LoadMapConfig`,
/// which is absent outside an active load.
pub fn creating_new_map(config: Res<LoadMapConfig>) -> bool {
    matches!(config.source, MapSource::New(_))
}

/// Requests a map load; if a response is requested, reports rejection or completion.
#[derive(Event)]
pub struct LoadGameSignal(pub LoadMapConfig);
impl LoadGameSignal {
    /// Loads the quick save, `test_save.dwd`.
    #[log_tags(Tag::GameLoad)]
    fn emit_load_quicksave(mut commands: Commands) {
        #[debug_player("Quicksave load requested")]
        commands.trigger(LoadGameSignal(LoadMapConfig::file("test_save.dwd")));
    }

    /// Rejects conflicting requests before changing the current map. Accepted file loads run
    /// migrations before starting the loader stages.
    #[log_tags(Tag::GameLoad)]
    fn on_load_game_signal_do_so(
        trigger: On<LoadGameSignal>,
        mut commands: Commands,
        mut next_game_state: ResMut<NextState<GameState>>,
        mut next_map_loading_stage: ResMut<NextState<MapLoadingStage>>,
        mut next_ui_state: ResMut<NextState<UiInteraction>>,
        current_game_state: Res<State<GameState>>,
        map_bound_entities: Query<Entity, With<MapBound>>,
    ) {
        let config = trigger.event().0.clone();

        #[warn_player("Map load already in progress — skipping")]
        if *current_game_state.get() == GameState::Loading {
            config.response.report(&mut commands, |entity| LoadGameReport { entity, result: LoadGameResult::AlreadyLoading });
            return;
        }
        #[warn_dev("Transition to {state:?} already queued — skipping")]
        if let NextState::Pending(state) | NextState::PendingIfNeq(state) = *next_game_state {
            config.response.report(&mut commands, |entity| LoadGameReport { entity, result: LoadGameResult::OtherTransitionAlreadyQueued { state } });
            return;
        }
        #[warn_player("Map file '{map_path}' not found — skipping")]
        if let MapSource::File(map_path) = &config.source && !std::path::Path::new(map_path).exists()
        {
            let path = map_path.clone();
            config.response.report(&mut commands, |entity| LoadGameReport { entity, result: LoadGameResult::MapNotFound { path } });
            return;
        }

        match &config.source {
            #[info_dev("Loading '{map_path}'")]
            MapSource::File(map_path) => {
                // A new map skips this: opening a SQLite connection would create a file.
                with_db_connection(map_path, Migrations::Apply, |_| Ok(()))
                    .expect("Failed to run migrations on map load");
            }
            MapSource::New(map_info) => info_dev!("Creating new map '{}'", map_info.name),
        }

        commands.queue(InitLoadRunner);

        commands.insert_resource(config);
        next_game_state.set(GameState::Loading);
        next_map_loading_stage.set(MapLoadingStage::Init);
        next_ui_state.set(UiInteraction::Free);

        map_bound_entities.iter().for_each(|entity| commands.entity(entity).despawn());
    }
}

/// Outcome of a `LoadGameSignal`: sent once, on rejection or when the game leaves `Loading`.
#[derive(EntityEvent, Serialize)]
pub struct LoadGameReport {
    #[serde(skip)]
    pub entity: Entity,
    pub result: LoadGameResult,
}

#[derive(Serialize)]
pub enum LoadGameResult {
    /// The map is loaded and `game_start_state` is in effect.
    Loaded { map: MapInfo, game_start_state: GameState },
    /// Another load is in progress.
    AlreadyLoading,
    /// A `GameState` transition is already queued; loading would overwrite it.
    OtherTransitionAlreadyQueued { state: GameState },
    /// The map file does not exist.
    MapNotFound { path: String },
}

/// All loader stages have finished; transition to the requested play and admin states.
#[log_tags(Tag::GameLoad)]
fn on_map_load_ready(
    load_config: Res<LoadMapConfig>,
    mut next_admin_mode: ResMut<NextState<AdminMode>>,
    mut next_game_state: ResMut<NextState<GameState>>,
) {
    #[info_player("Game loaded")]
    next_game_state.set(load_config.game_start_state);
    (*next_admin_mode).set_if_neq(load_config.admin_mode);
}

/// On leaving `Loading`, report the map and release the load config and runner.
fn finish_map_load(
    mut commands: Commands,
    load_config: Res<LoadMapConfig>,
    map_info: Res<MapInfo>,
) {
    let game_start_state = load_config.game_start_state;
    load_config.response.report(&mut commands, |entity| LoadGameReport {
        entity,
        result: LoadGameResult::Loaded { map: map_info.clone(), game_start_state },
    });
    commands.remove_resource::<LoadMapConfig>();
    commands.remove_resource::<LoadRunner>();
}
