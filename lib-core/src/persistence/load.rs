use crate::lib_prelude::*;
use bevy::input::common_conditions::input_just_released;

pub struct MapLoadPlugin;
impl Plugin for MapLoadPlugin {
    fn build(&self, app: &mut App) {
        app
            .init_resource::<DbEntityMap>()
            .init_resource::<GameLoadRegistry>()
            .add_systems(OnEnter(MapLoadingStage::LoadMapInfo), spawn_loading_tasks)
            .add_systems(OnEnter(MapLoadingStage::LoadResources), spawn_loading_tasks)
            .add_systems(OnEnter(MapLoadingStage::SpawnMapElements), spawn_loading_tasks)
            .add_systems(OnEnter(MapLoadingStage::Ready), |mut commands: Commands, mut next_game_state: ResMut<NextState<GameState>>| { 
                commands.trigger(DynamicGameEvent::game_started());
                next_game_state.set(GameState::Running); 
            })
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

#[derive(Event)]
pub struct LoadGameSignal(pub String);
impl LoadGameSignal {
    fn emit(
        mut commands: Commands,
    ) {
        println!("Loading game");
        commands.trigger(LoadGameSignal("test_save.dwd".into()));
    }
    fn on_trigger(
        trigger: On<LoadGameSignal>,
        mut commands: Commands,
        mut next_game_state: ResMut<NextState<GameState>>,
        mut next_ui_state: ResMut<NextState<UiInteraction>>,
        mut next_map_loading_stage: ResMut<NextState<MapLoadingStage>>,
        mut save_executor: ResMut<GameSaveExecutor>,
        map_bound_entities: Query<Entity, With<MapBound>>,
    ) {
        save_executor.save_name = trigger.event().0.clone();
        
        // Run migrations synchronously on main thread before parallel loading starts
        GameDbConnection::with_db_connection(&save_executor.save_name, |conn| {
            //conn.execute("DELETE FROM refinery_schema_history;", []); // Used to clear rafinery migrations history, uncomment when in need.
            db_migrations::migrations::runner().run(conn)?;
            Ok(())
        }).expect("Failed to run migrations on load");
        
        next_game_state.set(GameState::Loading);
        next_map_loading_stage.set(MapLoadingStage::Init);
        next_ui_state.set(UiInteraction::Free);

        // Despawn all existing map elements
        map_bound_entities.iter().for_each(|entity| commands.entity(entity).despawn());
    }
}

#[derive(Resource, Default)]
pub struct DbEntityMap {
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
    pub entity_map: &'a DbEntityMap,
    pub pagination: Pagination,
}
impl<'a, 'w, 's> LoadContext<'a, 'w, 's> {
    pub fn get_new_entity_for_old(&self, old_id: i64) -> Option<Entity> {
        self.entity_map.map.get(&old_id).copied()
    }
}

pub type LoaderFn = fn(&mut LoadContext) -> rusqlite::Result<LoadResult>;

#[derive(Resource, Default)]
pub struct GameLoadRegistry {
    pub loaders: HashMap<MapLoadingStage, Vec<LoaderFn>>,
}

impl GameLoadRegistry {
    pub fn register<T: Loadable>(&mut self, phase: MapLoadingStage) {
        self.loaders.entry(phase).or_default().push(T::load);
    }
}

#[derive(Component, Clone)]
pub struct DbLoadingTask {
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
        
        println!("Populated EntityMap with {} entities", count);
        ctx.commands.insert_resource(DbEntityMap { map });
        Ok(LoadResult::Finished)
    }
}

pub fn process_loading_tasks_system(
    par_commands: ParallelCommands,
    save_executor: Res<GameSaveExecutor>,
    entity_map: Res<DbEntityMap>,
    mut tasks: Query<(Entity, &mut DbLoadingTask)>,
) {
    let start_time = std::time::Instant::now();
    let time_budget = std::time::Duration::from_millis(5);

    tasks.par_iter_mut().for_each(|(entity, mut task)| {
        par_commands.command_scope(|mut cmd| {
             let _ = GameDbConnection::with_db_connection(&save_executor.save_name, |conn| {
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
                             eprintln!("Loading task failed: {}", e);
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
    let next = stage.get().next();
    println!("All loading tasks completed, moving to next stage: {:?}", next);
    next_stage.set(next);
}

fn spawn_loading_tasks(
    mut commands: Commands,
    registry: Res<GameLoadRegistry>,
    stage: ResMut<State<MapLoadingStage>>,
) {
    println!("Spawning loading tasks for phase: {:?}", stage.get());
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
