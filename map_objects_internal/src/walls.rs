use bevy::prelude::*;

use almanach::{Almanach, AlmanachAppExt, WallInfo};
use game_core::prelude::{GridCoords, GridImprint, MapObject, SSS};
use grids::obstacles::GridStructureType;
use grids::placement::{annotate_non_empty, GridObjectPlacer, GridsCollectionParam, PlacementChannel, PlacementMode, PlacementValidity, PlaceRequest, RemoveRequest, validator_all_empty};
use logging::prelude::*;
use map_objects::prelude::{Wall, WallStyleKey, WallStyles};
use persistence::{
    prelude::{AppGameLoadSaveExtension, CollectSave, GameDbHelpers, LoadContext, SaveWriter},
    rusqlite,
};
use states::prelude::MapLoadingStage;

pub struct WallPlugin;
impl Plugin for WallPlugin {
    fn build(&self, app: &mut App) {
        let almanach_info = BuilderWall::almanach_info(app.world().resource::<AssetServer>());
        app
            .add_systems(CollectSave, collect_walls)
            .register_loader(MapLoadingStage::SpawnMapElements, "walls", load_walls)
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
}
impl BuilderWall {
    pub fn almanach_info(asset_server: &AssetServer) -> WallInfo {
        WallInfo {
            name: "Wall".to_string(),
            grid_imprint: WALL_GRID_IMPRINT,
            sprite: asset_server.load("map_objects/wall_4side.png"),
            validate: validator_all_empty,
            annotate: annotate_non_empty,
            placement: PlacementChannel::of::<Wall>().with_modes(PlacementMode::OnPress),
        }
    }

    pub fn new(grid_position: GridCoords) -> Self {
        Self { grid_position }
    }

    fn on_builder_add_spawn_wall(
        trigger: On<Add, BuilderWall>,
        mut commands: Commands,
        styles: Res<WallStyles>,
        builders: Query<&BuilderWall>,
    ) {
        let entity = trigger.entity;
        let Ok(builder) = builders.get(entity) else { return; };

        commands.entity(entity)
            .remove::<BuilderWall>()
            .insert((
            Transform::from_translation(builder.grid_position.to_world_position_centered(WALL_GRID_IMPRINT).extend(0.)),
            builder.grid_position,
            WALL_GRID_IMPRINT,
            Wall,
            WallStyleKey(styles.default_index()),
        ));
    }
}

fn collect_walls(
    walls: Query<(Entity, &GridCoords), With<Wall>>,
    mut save: SaveWriter,
) {
    if walls.is_empty() { return; }
    let rows: Vec<(i64, i32, i32)> = walls
        .iter()
        .map(|(entity, coords)| {
            (
                entity.index_u32() as i64,
                coords.x,
                coords.y,
            )
        })
        .collect();
    Log::debug().dev().tag(Tag::GameSave).message(format!("Saving {} walls", rows.len()));
    save.submit(move |tx| {
        for (id, gx, gy) in rows {
            tx.save_marker("walls", id)?;
            tx.save_grid_coords(id, GridCoords { x: gx, y: gy })?;
        }
        Ok(())
    });
}

fn load_walls(ctx: &mut LoadContext) -> rusqlite::Result<()> {
    let mut stmt = ctx.conn.prepare("SELECT id FROM walls")?;
    let mut rows = stmt.query([])?;
    while let Some(row) = rows.next()? {
        let old_id: i64 = row.get(0)?;
        let grid_position = ctx.conn.get_grid_coords(old_id)?;

        let Some(entity) = ctx.entity(old_id) else {
            Log::warn().dev().tag(Tag::GameLoad).message(format!("Wall with old ID {old_id} has no corresponding new entity"));
            continue;
        };
        ctx.insert(entity, BuilderWall::new(grid_position));
    }
    Ok(())
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
