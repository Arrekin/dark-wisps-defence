use bevy::prelude::*;

use almanach::{Almanach, AlmanachAppExt, WallInfo};
use game_core::prelude::{GridCoords, GridImprint, MapObject, SSS, Z_OBSTACLE};
use grids::obstacles::GridStructureType;
use grids::placement::{annotate_non_empty, GridsCollectionParam, PlacementValidity, validator_all_empty};
use grids::prelude::{GridObjectPlacer, PlacementEmitter, PlacementMode, PlaceRequest, RemoveRequest};
use logging::prelude::*;
use map_objects::Wall;
use persistence::prelude::{AppGameLoadSaveExtension, GameDbHelpers, Loadable, LoadContext, LoadResult, Saveable, SaveableBatchCommand};
use persistence::rusqlite;
use states::prelude::MapLoadingStage;

pub struct WallPlugin;
impl Plugin for WallPlugin {
    fn build(&self, app: &mut App) {
        let almanach_info = BuilderWall::almanach_info(app.world().resource::<AssetServer>());
        app
            .add_systems(Update, pulsate_brightness)
            .register_db_loader::<BuilderWall>(MapLoadingStage::SpawnMapElements)
            .register_db_saver(BuilderWall::on_game_save_collect_walls)
            .add_observer(BuilderWall::on_builder_add_spawn_wall)
            .add_observer(on_wall_place_request_do_so)
            .add_observer(on_wall_remove_request_do_so)
            .register_walls(almanach_info)
            ;
    }
}

const WALL_GRID_IMPRINT: GridImprint = GridImprint::Rectangle { width: 1, height: 1 };

#[derive(Component, SSS)]
pub(crate) struct BuilderWall {
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
                Log::warn().dev().tag(Tag::GameLoad).message(format!("Wall with old ID {old_id} has no corresponding new entity"));
            }
        }

        let batch_size = batch.len();
        ctx.commands.insert_batch(batch);

        Ok(batch_size.into())
    }
}
impl BuilderWall {
    pub fn almanach_info(asset_server: &AssetServer) -> WallInfo {
        WallInfo {
            name: "Wall".to_string(),
            grid_imprint: WALL_GRID_IMPRINT,
            sprite: asset_server.load("map_objects/wall_4side.png"),
            validate: validator_all_empty,
            annotate: annotate_non_empty,
            place_emitter: || -> Box<dyn PlacementEmitter> { Box::new(PlaceRequest::<Wall>::default()) },
            remove_emitter: Some(|| -> Box<dyn PlacementEmitter> { Box::new(RemoveRequest::<Wall>::default()) }),
            begin_placing_emitter: None,
            place_mode: PlacementMode::OnPress,
            remove_mode: PlacementMode::OnPress,
        }
    }

    pub fn new(grid_position: GridCoords) -> Self {
        Self { grid_position, entity: None }
    }
    pub fn new_for_saving(grid_position: GridCoords, entity: Entity) -> Self {
        Self { grid_position, entity: Some(entity) }
    }

    fn on_builder_add_spawn_wall(
        trigger: On<Add, BuilderWall>,
        mut commands: Commands,
        almanach: Res<Almanach>,
        builders: Query<&BuilderWall>,
    ) {
        let entity = trigger.entity;
        let Ok(builder) = builders.get(entity) else { return; };

        commands.entity(entity)
            .remove::<BuilderWall>()
            .insert((
            Sprite {
                image: almanach.walls.sprite.clone(),
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

    fn on_game_save_collect_walls(
        mut commands: Commands,
        walls: Query<(Entity, &GridCoords), With<Wall>>,
    ) {
        Log::debug().dev().tag(Tag::GameSave).message(format!("Saving {} walls", walls.iter().count()));
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

fn on_wall_place_request_do_so(
    _trigger: On<PlaceRequest<Wall>>,
    mut commands: Commands,
    almanach: Res<Almanach>,
    mut grids: GridsCollectionParam,
    placer: Single<(&GridCoords, &GridImprint), With<GridObjectPlacer>>,
) {
    let (coords, grid_imprint) = placer.into_inner();
    let validity = (almanach.walls.validate)(MapObject::Wall, *coords, *grid_imprint, &grids);
    if validity == PlacementValidity::Invalid { return; }
    commands.spawn(BuilderWall::new(*coords));
    grids.reserved_coords.reserve(*coords, *grid_imprint);
}

fn on_wall_remove_request_do_so(
    _trigger: On<RemoveRequest<Wall>>,
    mut commands: Commands,
    grids: GridsCollectionParam,
    placer: Single<&GridCoords, With<GridObjectPlacer>>,
) {
    let coords = placer.into_inner();
    if let GridStructureType::Wall(entity) = grids.obstacle_grid[*coords].structure {
        commands.entity(entity).despawn();
    }
}
