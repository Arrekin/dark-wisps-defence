use lib_core::map_objects::Wall;
use lib_grid::grids::obstacles::{GridStructureType, ObstacleGrid, ReservedCoords};

use crate::prelude::*;
use crate::ui::grid_object_placer::GridObjectPlacer;

pub struct WallPlugin;
impl Plugin for WallPlugin {
    fn build(&self, app: &mut App) {
        app
            .add_systems(Update, pulsate_brightness)
            .register_db_loader::<BuilderWall>(MapLoadingStage::SpawnMapElements)
            .register_db_saver(BuilderWall::on_game_save)
            .add_observer(BuilderWall::on_add)
            .add_observer(on_wall_place_request)
            .add_observer(on_wall_remove_request);
    }
}

pub const WALL_GRID_IMPRINT: GridImprint = GridImprint::Rectangle { width: 1, height: 1 };
pub const WALL_BASE_IMAGE: &str = "map_objects/wall_4side.png";

#[derive(Component, SSS)]
pub struct BuilderWall {
    pub grid_position: GridCoords,
    pub entity: Option<Entity>,
}
impl Saveable for BuilderWall {
    fn save(self, tx: &rusqlite::Transaction) -> rusqlite::Result<()> {
        let entity_index = self.entity.expect("BuilderWall for saving purpose must have an entity").index_u32() as i64;

        tx.save_marker("walls", entity_index)?;
        tx.save_grid_coords(entity_index, self.grid_position)?;
        Ok(())
    }
}
impl Loadable for BuilderWall {
    fn load(ctx: &mut LoadContext) -> rusqlite::Result<LoadResult> {
        let mut stmt = ctx.conn.prepare_cached("SELECT id FROM walls LIMIT ?1 OFFSET ?2")?;

        let mut rows = stmt.query(ctx.pagination.as_params())?;

        let mut batch = Vec::new();
        while let Some(row) = rows.next()? {
            let old_id: i64 = row.get(0)?;
            let grid_position = ctx.conn.get_grid_coords(old_id)?;
            
            if let Some(new_entity) = ctx.get_new_entity_for_old(old_id) {
                batch.push((new_entity, BuilderWall::new_for_saving(grid_position, new_entity)));
            } else {
                eprintln!("Warning: Wall with old ID {} has no corresponding new entity", old_id);
            }
        }
        
        let batch_size = batch.len();
        ctx.commands.insert_batch(batch);
        
        Ok(batch_size.into())
    }
}
impl BuilderWall {
    pub fn new(grid_position: GridCoords) -> Self { 
        Self { grid_position, entity: None }
    }
    pub fn new_for_saving(grid_position: GridCoords, entity: Entity) -> Self { 
        Self { grid_position, entity: Some(entity) }
    }

    fn on_add(
        trigger: On<Add, BuilderWall>,
        mut commands: Commands,
        asset_server: Res<AssetServer>,
        builders: Query<&BuilderWall>,
    ) {
        let entity = trigger.entity;
        let Ok(builder) = builders.get(entity) else { return; };
        
        commands.entity(entity)
            .remove::<BuilderWall>()
            .insert((
                Sprite {
                    image: asset_server.load(WALL_BASE_IMAGE),
                    color: Color::hsla(0., 0., 1.5, 0.9), //for hdr brightness pulsation
                    custom_size: Some(WALL_GRID_IMPRINT.world_size()),
                    ..default()
                },
                Transform::from_translation(builder.grid_position.to_world_position_centered(WALL_GRID_IMPRINT).extend(Z_OBSTACLE)),
                builder.grid_position,
                WALL_GRID_IMPRINT,
                Wall,
            ));
    }

    fn on_game_save(
        mut commands: Commands,
        walls: Query<(Entity, &GridCoords), With<Wall>>,
    ) {
        println!("Creating batch of BuilderWalls for saving. {} walls", walls.iter().count());
        let batch = walls
            .iter()
            .map(|(entity, grid_coords)| BuilderWall::new_for_saving(*grid_coords, entity))
            .collect::<SaveableBatchCommand<_>>();
        commands.queue(batch);
    }
}

fn pulsate_brightness(
    time: Res<Time>,
    mut walls: Query<&mut Sprite, With<Wall>>,
    mut lightness_rising: Local<bool>,
    mut shared_lightness: Local<f32>,
) {
    let lightness_delta = time.delta_secs() / 10.;
    *shared_lightness += if *lightness_rising { lightness_delta } else { -lightness_delta };
    if *shared_lightness > 1.5 {
        *lightness_rising = false;
    } else if *shared_lightness < 1. {
        *lightness_rising = true;
        *shared_lightness = 1.;
    }
    for mut sprite in walls.iter_mut() {
        if let Color::Hsla(Hsla{lightness, ..}) = &mut sprite.color {
            *lightness = *shared_lightness;
        }
    }
}

fn on_wall_place_request(
    _trigger: On<lib_core::placement::PlaceRequest<Wall>>,
    mut commands: Commands,
    mut reserved_coords: ResMut<ReservedCoords>,
    obstacle_grid: Res<ObstacleGrid>,
    placer: Single<(&GridObjectPlacer, &GridCoords, &GridImprint)>,
) {
    let (grid_object_placer, coords, grid_imprint) = placer.into_inner();
    let Some(active_placement) = &grid_object_placer.active_placement else { return };
    if !matches!(active_placement.map_object, MapObject::Wall) { return };
    
    if !coords.is_in_bounds(obstacle_grid.bounds()) { return; }
    if obstacle_grid[*coords].is_empty() && !reserved_coords.any_reserved(*coords, *grid_imprint) {
        commands.spawn(BuilderWall::new(*coords));
        reserved_coords.reserve(*coords, *grid_imprint);
    }
}

fn on_wall_remove_request(
    _trigger: On<lib_core::placement::RemoveRequest<Wall>>,
    mut commands: Commands,
    obstacle_grid: Res<ObstacleGrid>,
    placer: Single<&GridCoords, With<GridObjectPlacer>>,
) {
    let coords = placer.into_inner();
    if let GridStructureType::Wall(entity) = obstacle_grid[*coords].structure {
        commands.entity(entity).despawn();
    }
}
