use strum::{AsRefStr, EnumIter};
use serde::{Deserialize, Serialize};

use lib_grid::grids::obstacles::ObstacleGrid;

use crate::prelude::*;
use super::spawning::BuilderWisp;

pub struct SummoningPlugin;
impl Plugin for SummoningPlugin {
    fn build(&self, app: &mut App) {
        app
            .add_systems(OnEnter(MapLoadingStage::LoadResources), |mut commands: Commands| { commands.insert_resource(SummoningClock::default());})
            .add_systems(Update, tick_active_summoning_system.run_if(in_state(GameState::Running)))
            .add_observer(on_summoning_activation_event)
            .add_observer(BuilderSummoning::on_add)
            .register_db_loader::<BuilderSummoning>(MapLoadingStage::LoadResources)
            .register_db_loader::<SummoningClock>(MapLoadingStage::LoadResources)
            .register_db_saver(BuilderSummoning::on_game_save)
            .register_db_saver(SummoningClock::on_game_save);
    }
}

#[derive(Component, Clone, Debug, Serialize, Deserialize)]
#[require(MapBound, SummoningRuntime)]
pub struct Summoning {
    pub id_name: String,
    pub wisp_types: Vec<WispType>,
    pub area: SpawnArea,
    pub tempo: SpawnTempo,
    pub limit_count: Option<i32>,
    pub activation_event: String,
}
impl Default for Summoning {
    fn default() -> Self {
        Self {
            id_name: "new_summoning".to_string(),
            wisp_types: vec![WispType::Fire],
            area: SpawnArea::default(),
            tempo: SpawnTempo::default(),
            limit_count: None,
            activation_event: "game-started".to_string(),
        }
    }
}
impl Summoning {
    fn get_random_wisp_type(&self, rng: &mut nanorand::tls::TlsWyRand) -> WispType {
        self.wisp_types[rng.generate_range(0..self.wisp_types.len())]
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, EnumIter, AsRefStr)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SpawnArea {
    Coords { coords: Vec<GridCoords> },
    Rect { origin: GridCoords, width: i32, height: i32 },
    Edge { side: EdgeSide },
    #[default]
    EdgesAll,
}

impl SpawnArea {
    fn get_random_coord(
        &self,
        obstacle_grid: &ObstacleGrid,
        rng: &mut nanorand::tls::TlsWyRand,
    ) -> GridCoords {
        // Nano-rand is off by 1 in i32! 
        match self {
            SpawnArea::Coords { coords } => {
                let idx = rng.generate_range(0..coords.len());
                coords[idx]
            }
            SpawnArea::Rect { origin, width, height } => {
                let x = origin.x + rng.generate_range(1..=*width);
                let y = origin.y + rng.generate_range(1..=*height);
                GridCoords { x, y }
            }
            SpawnArea::Edge { side } => {
                let (width, height) = obstacle_grid.bounds();
                match side {
                    EdgeSide::Top => GridCoords { x: rng.generate_range(1..=width), y: height - 1 },
                    EdgeSide::Bottom => GridCoords { x: rng.generate_range(1..=width), y: 0 },
                    EdgeSide::Left => GridCoords { x: 0, y: rng.generate_range(1..=height) },
                    EdgeSide::Right => GridCoords { x: width - 1, y: rng.generate_range(1..=height) },
                }
            }
            SpawnArea::EdgesAll => {
                let (width, height) = obstacle_grid.bounds();
                let edge_count = 2 * (width + height - 2);
                let edge_idx = rng.generate_range(1..=edge_count);
                
                if edge_idx < width {
                    GridCoords { x: edge_idx, y: 0 }
                } else if edge_idx < width + height - 1 {
                    GridCoords { x: width - 1, y: edge_idx - width + 1 }
                } else if edge_idx < 2 * width + height - 2 {
                    GridCoords { x: width - 1 - (edge_idx - width - height + 1), y: height - 1 }
                } else {
                    GridCoords { x: 0, y: height - 1 - (edge_idx - 2 * width - height + 2) }
                }
            }
        }
    }
}

#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, EnumIter, AsRefStr)]
#[serde(rename_all = "snake_case")]
pub enum EdgeSide {
    #[default]
    Top,
    Bottom,
    Left,
    Right,
}

#[derive(Copy, Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SpawnTempo {
    /// Spawn `count` wisps every `seconds` (optional jitter). If `count` omitted -> 1.
    Continuous { seconds: f32, #[serde(default)] jitter: f32, #[serde(default = "default_one")] bulk_count: i32 },
}

impl Default for SpawnTempo {
    fn default() -> Self {
        Self::Continuous { seconds: 1.0, jitter: 0.0, bulk_count: 1 }
    }
}

fn default_one() -> i32 { 1 }

// --------------- SUMMONING ENTITIES AND RUNTIME ---------------
#[derive(Component, Default)]
pub struct SummoningMarkerActive;

#[derive(Component, Default)]
struct SummoningRuntime {
    produced: i32,
    next_spawn_time: f32,
}

#[derive(Resource, Default, Clone, SSS)]
struct SummoningClock(f32);
impl Saveable for SummoningClock {
    fn save(self, tx: &rusqlite::Transaction) -> rusqlite::Result<()> {
        tx.save_stat("summoning_clock", self.0)?;
        Ok(())
    }
}

impl Loadable for SummoningClock {
    fn load(ctx: &mut LoadContext) -> rusqlite::Result<LoadResult> {
        let clock_value = ctx.conn.get_stat("summoning_clock").unwrap_or(0.0);
        ctx.commands.insert_resource(SummoningClock(clock_value));
        Ok(LoadResult::Finished)
    }
}
impl SummoningClock {
    fn on_game_save(
        mut commands: Commands,
        clock: Res<SummoningClock>,
    ) {
        commands.queue(SaveableBatchCommand::from_single(clock.clone()));
    }
}


// --------------- BUILDER PATTERN FOR SAVE/LOAD ---------------

#[derive(Clone, Copy, Debug)]
pub struct SummoningSaveData {
    pub entity: Entity,
    pub produced: i32,
    pub next_spawn_time: f32,
    pub is_active: bool,
}

#[derive(Component, SSS)]
pub struct BuilderSummoning {
    pub summoning: Summoning,
    pub save_data: Option<SummoningSaveData>,
}
impl Saveable for BuilderSummoning {
    fn save(self, tx: &rusqlite::Transaction) -> rusqlite::Result<()> {
        let save_data = self.save_data.expect("BuilderSummoning for saving must have save_data");
        let entity_index = save_data.entity.index_u32() as i64;

        tx.register_entity(entity_index)?;
        
        let summoning_json = serde_json::to_string(&self.summoning)
            .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?;
        
        tx.execute(
            "INSERT OR REPLACE INTO summonings (id, summoning_json, produced, next_spawn_time, is_active) VALUES (?1, ?2, ?3, ?4, ?5)",
            (entity_index, summoning_json, save_data.produced, save_data.next_spawn_time, if save_data.is_active { 1 } else { 0 }),
        )?;
        Ok(())
    }
}
impl Loadable for BuilderSummoning {
    fn load(ctx: &mut LoadContext) -> rusqlite::Result<LoadResult> {
        let mut stmt = ctx.conn.prepare("SELECT id, summoning_json, produced, next_spawn_time, is_active FROM summonings LIMIT ?1 OFFSET ?2")?;
        let mut rows = stmt.query(ctx.pagination.as_params())?;
        
        let mut count = 0;
        while let Some(row) = rows.next()? {
            let old_id: i64 = row.get(0)?;
            let summoning_json: String = row.get(1)?;
            let produced: i32 = row.get(2)?;
            let next_spawn_time: f32 = row.get(3)?;
            let is_active: i32 = row.get(4)?;
            
            let summoning: Summoning = serde_json::from_str(&summoning_json)
                .map_err(|e| rusqlite::Error::FromSqlConversionFailure(1, rusqlite::types::Type::Text, Box::new(e)))?;
            
            if let Some(new_entity) = ctx.get_new_entity_for_old(old_id) {
                let save_data = SummoningSaveData {
                    entity: new_entity,
                    produced,
                    next_spawn_time,
                    is_active: is_active != 0,
                };
                ctx.commands.entity(new_entity).insert(BuilderSummoning::new_for_saving(summoning, save_data));
            } else {
                Log::warn().dev().tag(Tag::GameLoad).message(format!("Summoning with old ID {old_id} has no corresponding new entity"));
            }
            count += 1;
        }
        Ok(count.into())
    }
}
impl BuilderSummoning {
    pub fn new(summoning: Summoning) -> Self {
        Self { summoning, save_data: None }
    }
    
    pub fn new_for_saving(summoning: Summoning, save_data: SummoningSaveData) -> Self {
        Self { summoning, save_data: Some(save_data) }
    }
    
    fn on_game_save(
        mut commands: Commands,
        summonings: Query<(Entity, &Summoning, &SummoningRuntime, Has<SummoningMarkerActive>)>,
    ) {
        if summonings.is_empty() { return; }
        let batch = summonings.iter().map(|(entity, summoning, runtime, is_active)| {
            let save_data = SummoningSaveData {
                entity,
                produced: runtime.produced,
                next_spawn_time: runtime.next_spawn_time,
                is_active,
            };
            BuilderSummoning::new_for_saving(summoning.clone(), save_data)
        }).collect::<SaveableBatchCommand<_>>();
        commands.queue(batch);
    }
    
    fn on_add(
        trigger: On<Add, BuilderSummoning>,
        mut commands: Commands,
        builders: Query<&BuilderSummoning>,
    ) {
        let entity = trigger.entity;
        let Ok(builder) = builders.get(entity) else { return; };
        
        let mut entity_commands = commands.entity(entity);
        
        // Restore runtime state if loading from save
        if let Some(save_data) = &builder.save_data {
            entity_commands.insert(SummoningRuntime {
                produced: save_data.produced,
                next_spawn_time: save_data.next_spawn_time,
            });
            
            if save_data.is_active {
                entity_commands.insert(SummoningMarkerActive);
            }
        }
        
        // Insert the actual Summoning component and remove builder
        entity_commands
            .remove::<BuilderSummoning>()
            .insert(builder.summoning.clone());
    }
}

fn tick_active_summoning_system(
    time: Res<Time>,
    mut clock: ResMut<SummoningClock>,
    mut commands: Commands,
    obstacle_grid: Res<ObstacleGrid>,
    mut summoning: Query<(&Summoning, &mut SummoningRuntime), With<SummoningMarkerActive>>,
) {
    clock.0 += time.delta_secs();
    let now = clock.0;
    let mut rng = nanorand::tls_rng();

    for (summoning, mut runtime) in &mut summoning {
        // Check if limit is reached(if set)
        let remaining = summoning.limit_count.map(|m| m.saturating_sub(runtime.produced)).unwrap_or(i32::MAX);
        if remaining <= 0 { continue; }

        // Wait until due
        if now < runtime.next_spawn_time { continue; }

        match summoning.tempo {
            SpawnTempo::Continuous { seconds, jitter, bulk_count } => {
                let to_spawn: i32 = std::cmp::min(bulk_count, remaining);
                if to_spawn <= 0 { continue; }
                for _ in 0..(to_spawn as usize) {
                    let grid_coords = summoning.area.get_random_coord(&obstacle_grid, &mut rng);
                    let wisp_type = summoning.get_random_wisp_type(&mut rng);
                    commands.spawn(BuilderWisp::new(wisp_type, grid_coords));
                }
                runtime.produced = runtime.produced.saturating_add(to_spawn);
                let j = if jitter > 0.0 { (rng.generate::<f32>() * 2.0 - 1.0) * jitter } else { 0.0 };
                runtime.next_spawn_time = now + (seconds + j);
            }
        }
    }
}

fn on_summoning_activation_event(
    trigger: On<DynamicGameEvent>,
    mut commands: Commands,
    summonings: Query<(Entity, &Summoning), Without<SummoningMarkerActive>>,
) {
    let event = &trigger.event().0;
    for (entity, summoning) in summonings.iter() {
        if event != &summoning.activation_event { continue; }
        commands.entity(entity).insert(SummoningMarkerActive);
        Log::info().player().tag(Tag::Wave).message(format!("Summoning '{}' activated", summoning.id_name));
    }
}