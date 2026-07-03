use bevy::{
    input::common_conditions::input_just_released,
    platform::collections::HashMap,
    prelude::*,
};

use game_core::prelude::*;
use logging::prelude::*;
use states::{AdminMode, prelude::*};

use crate::{
    common::{AppGameLoadSaveExtension, db_migrations, with_db_connection},
    save::GameSaveExecutor,
};

pub struct MapLoadPlugin;
impl Plugin for MapLoadPlugin {
    fn build(&self, app: &mut App) {
        app
            .init_resource::<DbEntityMap>()
            .init_resource::<GameLoadRegistry>()
            .init_resource::<GameMapList>()
            .add_systems(OnEnter(MapLoadingStage::LoadMapInfo), spawn_loading_tasks)
            .add_systems(OnEnter(MapLoadingStage::LoadResources), spawn_loading_tasks)
            .add_systems(OnEnter(MapLoadingStage::SpawnMapElements), spawn_loading_tasks)
            .add_systems(OnEnter(MapLoadingStage::SpawnEffectInstances), spawn_loading_tasks)
            .add_systems(OnEnter(MapLoadingStage::Ready), on_map_load_ready)
            .add_systems(Update, (
                progress_map_loading_state.run_if(in_state(GameState::Loading)),
                process_loading_tasks_system,
                LoadGameSignal::emit.run_if(input_just_released(KeyCode::KeyA)),
            ))
            .add_observer(LoadGameSignal::on_trigger)
            .register_db_loader::<PopulateDbEntityMapTask>(MapLoadingStage::LoadMapInfo);
    }
}

pub trait Loadable {
    fn load(ctx: &mut LoadContext) -> rusqlite::Result<LoadResult>;
}

/// All .dwd map files found in the maps/ directory at startup.
#[derive(Resource)]
pub struct GameMapList {
    pub names: Vec<String>,
}
impl Default for GameMapList {
    fn default() -> Self {
        let names = std::fs::read_dir("maps")
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
        Self { names }
    }
}
impl GameMapList {
    pub fn paths(&self) -> Vec<String> {
        self.names.iter().map(|name| format!("maps/{}.dwd", name)).collect()
    }
}

#[derive(Resource, Clone)]
pub struct LoadMapConfig {
    pub map_path: String,
    pub game_start_state: GameState,
    pub admin_mode: AdminMode,
}
impl LoadMapConfig {
    pub fn new(map_path: impl Into<String>) -> Self {
        Self {
            map_path: map_path.into(),
            game_start_state: GameState::Running,
            admin_mode: AdminMode::Disabled,
        }
    }
}

#[derive(Event)]
pub struct LoadGameSignal(pub LoadMapConfig);
impl LoadGameSignal {
    fn emit(mut commands: Commands) {
        Log::debug().dev().tag(Tag::GameLoad).message("Triggering load signal");
        commands.trigger(LoadGameSignal(LoadMapConfig::new("test_save.dwd")));
    }
    fn on_trigger(
        trigger: On<LoadGameSignal>,
        mut commands: Commands,
        mut save_executor: ResMut<GameSaveExecutor>,
        mut next_game_state: ResMut<NextState<GameState>>,
        mut next_map_loading_stage: ResMut<NextState<MapLoadingStage>>,
        mut next_ui_state: ResMut<NextState<UiInteraction>>,
        map_bound_entities: Query<Entity, With<MapBound>>,
    ) {
        let config = trigger.event().0.clone();
        save_executor.save_name = config.map_path.clone();
        Log::info().dev().tag(Tag::GameLoad).message(format!("Loading '{}'", config.map_path));

        // Run migrations synchronously on main thread before parallel loading starts
        with_db_connection(&config.map_path, |conn| {
            //conn.execute("DELETE FROM refinery_schema_history;", [])?; // Used to clear refinery migrations history, uncomment when in need.
            db_migrations::migrations::runner().run(conn)?;
            Ok(())
        }).expect("Failed to run migrations on load");

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
    commands.trigger(DynamicGameEvent::game_started());
    next_game_state.set(load_config.game_start_state);
    (*next_admin_mode).set_if_neq(load_config.admin_mode);
    commands.remove_resource::<LoadMapConfig>();
}

#[derive(Resource, Default)]
pub(crate) struct DbEntityMap {
    pub map: HashMap<i64, Entity>,
}

#[derive(Clone, Copy, Debug)]
pub struct Pagination {
    pub limit: usize,
    pub offset: usize,
}
impl Pagination {
    pub fn as_params(&self) -> [usize; 2] {
        [self.limit, self.offset]
    }
}

pub enum LoadResult {
    Progressed(usize),
    Finished,
}
impl From<usize> for LoadResult {
    fn from(value: usize) -> Self {
        if value == 0 {
            LoadResult::Finished
        } else {
            LoadResult::Progressed(value)
        }
    }
}

pub struct LoadContext<'a, 'w, 's> {
    pub conn: &'a rusqlite::Connection,
    pub commands: &'a mut Commands<'w, 's>,
    pub(crate) entity_map: &'a DbEntityMap,
    pub pagination: Pagination,
}
impl<'a, 'w, 's> LoadContext<'a, 'w, 's> {
    pub fn get_new_entity_for_old(&self, old_id: i64) -> Option<Entity> {
        self.entity_map.map.get(&old_id).copied()
    }
}

pub(crate) type LoaderFn = fn(&mut LoadContext) -> rusqlite::Result<LoadResult>;

#[derive(Resource, Default)]
pub(crate) struct GameLoadRegistry {
    pub loaders: HashMap<MapLoadingStage, Vec<LoaderFn>>,
}
impl GameLoadRegistry {
    pub fn register<T: Loadable>(&mut self, phase: MapLoadingStage) {
        self.loaders.entry(phase).or_default().push(T::load);
    }
}

#[derive(Component, Clone)]
pub(crate) struct DbLoadingTask {
    pub loader: LoaderFn,
    pub pagination: Pagination,
}

struct PopulateDbEntityMapTask;
impl Loadable for PopulateDbEntityMapTask {
    fn load(ctx: &mut LoadContext) -> rusqlite::Result<LoadResult> {
        let mut map = HashMap::new();
        let mut stmt = ctx.conn.prepare("SELECT id FROM entities")?;
        let rows = stmt.query_map([], |row| row.get::<_, i64>(0))?;

        let mut count = 0;
        for row in rows {
            let old_id = row?;
            let new_entity = ctx.commands.spawn_empty().id();
            map.insert(old_id, new_entity);
            count += 1;
        }

        Log::debug().dev().tag(Tag::GameLoad).message(format!("EntityMap populated: {} entities", count));
        ctx.commands.insert_resource(DbEntityMap { map });
        Ok(LoadResult::Finished)
    }
}

pub(crate) fn process_loading_tasks_system(
    par_commands: ParallelCommands,
    entity_map: Res<DbEntityMap>,
    save_executor: Res<GameSaveExecutor>,
    mut tasks: Query<(Entity, &mut DbLoadingTask)>,
) {
    let start_time = std::time::Instant::now();
    let time_budget = std::time::Duration::from_millis(5);

    tasks.par_iter_mut().for_each(|(entity, mut task)| {
        par_commands.command_scope(|mut cmd| {
             let _ = with_db_connection(&save_executor.save_name, |conn| {
                 loop {
                     // Check global system budget
                     if start_time.elapsed() > time_budget {
                         break;
                     }

                     let mut ctx = LoadContext {
                         conn,
                         commands: &mut cmd,
                         entity_map: &entity_map,
                         pagination: task.pagination,
                     };

                     match (task.loader)(&mut ctx) {
                         Ok(LoadResult::Finished) => {
                             cmd.entity(entity).despawn();
                             break; // Task done
                         },
                         Ok(LoadResult::Progressed(count)) => {
                             task.pagination.offset += count;
                             // Continue loop to process more if time permits
                         },
                         Err(e) => {
                             Log::error().dev().tag(Tag::GameLoad).message(format!("Loading task failed: {e}"));
                             cmd.entity(entity).despawn(); // Stop on error
                             break;
                         }
                     }
                 }
                 Ok(())
             });
        });
    });
}

/// Check if there are any MapLoadingTasks (local) or LoadingTask (DB) left.
fn progress_map_loading_state(
    stage: Res<State<MapLoadingStage>>,
    mut next_stage: ResMut<NextState<MapLoadingStage>>,
    loading_tasks: Query<(), With<DbLoadingTask>>,
) {
    if !loading_tasks.is_empty() { return; }
    let Some(next) = stage.get().next() else { return; };
    Log::debug().dev().tag(Tag::GameLoad).message(format!("Stage complete, advancing to {next:?}"));
    next_stage.set(next);
}

fn spawn_loading_tasks(
    mut commands: Commands,
    registry: Res<GameLoadRegistry>,
    stage: ResMut<State<MapLoadingStage>>,
) {
    Log::debug().dev().tag(Tag::GameLoad).message(format!("Starting load phase {:?}", stage.get()));
    let target_phase = stage.get();
    if let Some(loaders) = registry.loaders.get(target_phase) {
        for loader in loaders {
            commands.spawn(DbLoadingTask {
                loader: *loader,
                pagination: Pagination { limit: 100, offset: 0 },
            });
        }
    }
}
