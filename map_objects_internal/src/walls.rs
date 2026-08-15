use bevy::prelude::*;

use almanach::{Almanach, AlmanachAppExt, WallInfo};
use game_core::prelude::{GridCoords, GridImprint, MapObject, SSS};
use grids::obstacles::GridStructureType;
use grids::placement::{annotate_non_empty, GridObjectPlacer, GridsCollectionParam, PlacementChannel, PlacementMode, PlacementStyle, PlacementValidity, PlaceRequest, RemoveRequest, validator_all_empty};
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

/// How a wall's style arrives. The placer knows the position it picked in the style table; a save
/// file knows the name that position had when it was written. Both resolve to the same
/// [`WallStyleKey`] when the wall spawns.
pub(crate) enum WallStyleSource {
    Key(WallStyleKey),
    Name(String),
}
impl From<PlacementStyle> for WallStyleSource {
    fn from(style: PlacementStyle) -> Self {
        Self::Key(style.into())
    }
}
impl From<String> for WallStyleSource {
    fn from(name: String) -> Self {
        Self::Name(name)
    }
}

#[derive(Component, SSS)]
pub(crate) struct BuilderWall {
    pub grid_position: GridCoords,
    pub style: WallStyleSource,
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

    pub fn new(grid_position: GridCoords, style: impl Into<WallStyleSource>) -> Self {
        Self { grid_position, style: style.into() }
    }

    fn on_builder_add_spawn_wall(
        trigger: On<Add, BuilderWall>,
        mut commands: Commands,
        builders: Query<&BuilderWall>,
        styles: Res<WallStyles>,
    ) {
        let entity = trigger.entity;
        let Ok(builder) = builders.get(entity) else { return; };

        let style = match &builder.style {
            WallStyleSource::Key(key) => *key,
            WallStyleSource::Name(name) => styles.key_of(name).unwrap_or_else(|| {
                Log::warn().dev().tag(Tag::GameLoad).message(format!("Wall style '{name}' is not in this map's table; drawing it with the default"));
                WallStyleKey::default()
            }),
        };

        commands.entity(entity)
            .remove::<BuilderWall>()
            .insert((
            Transform::from_translation(builder.grid_position.to_world_position_centered(WALL_GRID_IMPRINT).extend(0.)),
            builder.grid_position,
            WALL_GRID_IMPRINT,
            Wall,
            style,
        ));
    }
}

fn collect_walls(
    walls: Query<(Entity, &GridCoords, &WallStyleKey), With<Wall>>,
    styles: Res<WallStyles>,
    mut save: SaveWriter,
) {
    if walls.is_empty() { return; }
    let rows: Vec<(i64, i32, i32, String)> = walls
        .iter()
        .map(|(entity, coords, key)| {
            (
                entity.index_u32() as i64,
                coords.x,
                coords.y,
                // An out-of-range key means the style table shrank under a live wall. The empty
                // name loads back as the default, with a warn.
                styles.name_of(*key).unwrap_or_default().to_string(),
            )
        })
        .collect();
    Log::debug().dev().tag(Tag::GameSave).message(format!("Saving {} walls", rows.len()));
    save.submit(move |tx| {
        for (id, gx, gy, style) in rows {
            tx.register_entity(id)?;
            tx.execute(
                "INSERT OR REPLACE INTO walls (id, style) VALUES (?1, ?2)",
                rusqlite::params![id, style],
            )?;
            tx.save_grid_coords(id, GridCoords { x: gx, y: gy })?;
        }
        Ok(())
    });
}

fn load_walls(ctx: &mut LoadContext) -> rusqlite::Result<()> {
    let mut stmt = ctx.conn.prepare("SELECT id, style FROM walls")?;
    let mut rows = stmt.query([])?;
    while let Some(row) = rows.next()? {
        let old_id: i64 = row.get(0)?;
        let style: String = row.get(1)?;
        let grid_position = ctx.conn.get_grid_coords(old_id)?;

        let Some(entity) = ctx.entity(old_id) else {
            Log::warn().dev().tag(Tag::GameLoad).message(format!("Wall with old ID {old_id} has no corresponding new entity"));
            continue;
        };
        ctx.insert(entity, BuilderWall::new(grid_position, style));
    }
    Ok(())
}

fn on_wall_place_request_do_so(
    _trigger: On<PlaceRequest<Wall>>,
    mut commands: Commands,
    almanach: Res<Almanach>,
    mut grids: GridsCollectionParam,
    placer: Single<(&GridCoords, &GridImprint, &PlacementStyle), With<GridObjectPlacer>>,
) {
    let (coords, grid_imprint, placement_style) = placer.into_inner();
    let validity = (almanach.walls.validate)(MapObject::Wall, *coords, *grid_imprint, &grids);
    if validity == PlacementValidity::Invalid { return; }
    commands.spawn(BuilderWall::new(*coords, *placement_style));
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
