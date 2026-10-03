use bevy::prelude::*;

use almanach::{Almanach, AlmanachAppExt, ObjectPresentation, WallInfo};
use game_core::prelude::{GridCoords, GridImprint, MapObject, SSS};
use grids::{
    obstacles::GridStructureType,
    placement::{
        annotate_non_empty, GridObjectPlacer, GridsCollectionParam, PlacementModes, PlacementStyle, PlacementValidity,
        PlaceRequest, RemoveRequest, validator_all_empty,
    },
};
use hud::prelude::BuilderSideMenuItemTooltip;
use logging::prelude::*;
use map_objects::{
    prelude::{BuilderWallSideMenuTooltip, Wall},
    wall_style::{WallStyleKey, WallStyles},
};
use persistence::{prelude::*, rusqlite};
use states::prelude::MapLoadingStage;

pub(crate) struct WallPlugin;
impl Plugin for WallPlugin {
    fn build(&self, app: &mut App) {
        app
            .add_systems(CollectSave, collect_walls)
            .register_loader(MapLoadingStage::SpawnMapElements, "walls", load_walls)
            .add_observer(BuilderWall::on_builder_add_spawn_wall)
            .add_observer(on_wall_place_request_do_so)
            .add_observer(on_wall_remove_request_do_so)
            .add_observer(on_builder_add_spawn_wall_tooltip)
            .register_walls(BuilderWall::almanach_info());
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
    pub fn almanach_info() -> WallInfo {
        WallInfo {
            name: "Wall".to_string(),
            description: "Blocks ground movement and shapes the paths wisps take.".to_string(),
            grid_imprint: WALL_GRID_IMPRINT,
            validate: validator_all_empty,
            annotate: annotate_non_empty,
            placement: PlacementModes::on_press(),
            presentation: ObjectPresentation {
                tooltip: Some(wall_tooltip),
            },
        }
    }

    pub fn new(grid_position: GridCoords, style: impl Into<WallStyleSource>) -> Self {
        Self { grid_position, style: style.into() }
    }

    #[log_tags(Tag::GameLoad)]
    fn on_builder_add_spawn_wall(
        trigger: On<Add, BuilderWall>,
        mut commands: Commands,
        styles: Res<WallStyles>,
        builders: Query<&BuilderWall>,
    ) {
        let entity = trigger.entity;
        let Ok(builder) = builders.get(entity) else { return; };

        let style = match &builder.style {
            WallStyleSource::Key(key) => *key,
            WallStyleSource::Name(name) => styles.key_of(name).unwrap_or_else(|| {
                #[warn_dev("Wall style '{name}' is not in this map's table; drawing it with the default")]
                WallStyleKey::default()
            }),
        };

        commands.entity(entity)
            .remove::<BuilderWall>()
            .insert((
                builder.grid_position,
                WALL_GRID_IMPRINT,
                Wall,
                style,
            ));
    }
}

#[log_tags(Tag::GameSave)]
fn collect_walls(
    walls: Query<(Entity, &GridCoords, &WallStyleKey), With<Wall>>,
    styles: Res<WallStyles>,
    mut save: SaveWriter,
) {
    if walls.is_empty() { return; }

    #[debug_dev("Saving {} walls", rows.len())]
    let rows: Vec<(i64, GridCoords, String)> = walls
        .iter()
        .map(|(entity, coords, key)| {
            (
                entity.index_u32() as i64,
                *coords,
                // An out-of-range key means the style table shrank under a live wall. The empty
                // name loads back as the default, with a warn.
                styles.name_of(*key).unwrap_or_default().to_string(),
            )
        })
        .collect();
    save.submit(move |ctx| {
        for (id, coords, style) in rows {
            ctx.register_entity(id)?;
            ctx.tx.execute(
                "INSERT OR REPLACE INTO walls (id, style) VALUES (?1, ?2)",
                rusqlite::params![id, style],
            )?;
            ctx.save_grid_coords(id, coords)?;
        }
        Ok(())
    });
}

fn load_walls(ctx: &mut LoadContext) -> LoadResult {
    ctx.for_each_entity("SELECT id, style FROM walls", |ctx, old_id, entity, row| {
        let style: String = row.get(1)?;
        let grid_position = ctx.grid_coords(old_id)?;
        ctx.insert(entity, BuilderWall::new(grid_position, style));
        Ok(())
    })
}

#[log_tags(Tag::MapObjects)]
fn on_wall_place_request_do_so(
    trigger: On<PlaceRequest>,
    mut commands: Commands,
    almanach: Res<Almanach>,
    mut grids: GridsCollectionParam,
    placer: Single<(&GridCoords, &GridImprint, &PlacementStyle), With<GridObjectPlacer>>,
) {
    let PlaceRequest(MapObject::Wall) = *trigger else { return };
    let (coords, grid_imprint, placement_style) = placer.into_inner();
    let validity = (almanach.walls.validate)(MapObject::Wall, *coords, *grid_imprint, &grids);
    if validity == PlacementValidity::Invalid { return; }
    #[debug_dev("Wall placed at ({}, {})", coords.x, coords.y)]
    commands.spawn(BuilderWall::new(*coords, *placement_style));
    grids.reserved_coords.reserve(*coords, *grid_imprint);
}

#[log_tags(Tag::MapObjects)]
fn on_wall_remove_request_do_so(
    trigger: On<RemoveRequest>,
    mut commands: Commands,
    grids: GridsCollectionParam,
    placer: Single<&GridCoords, With<GridObjectPlacer>>,
) {
    let RemoveRequest(MapObject::Wall) = *trigger else { return };
    let coords = placer.into_inner();
    #[debug_dev("Wall removed at ({}, {})", coords.x, coords.y)]
    if let GridStructureType::Wall(entity) = grids.obstacle_grid[*coords].structure {
        commands.entity(entity).despawn();
    }
}

/// Queues tooltip construction for a wall placement tile.
pub(crate) fn wall_tooltip(commands: &mut Commands, anchor: Entity, _map_object: MapObject) {
    commands.spawn(BuilderWallSideMenuTooltip(anchor));
}

fn on_builder_add_spawn_wall_tooltip(
    trigger: On<Add, BuilderWallSideMenuTooltip>,
    mut commands: Commands,
    almanach: Res<Almanach>,
    builders: Query<&BuilderWallSideMenuTooltip>,
) {
    let entity = trigger.entity;
    let Ok(builder) = builders.get(entity) else { return; };

    let info = &almanach.walls;
    commands.entity(entity)
        .remove::<BuilderWallSideMenuTooltip>()
        .insert(
            BuilderSideMenuItemTooltip::new(builder.0)
                .with_name(info.name.clone())
                .with_description(info.description.clone())
                .with_fact(info.grid_imprint.label()),
        );
}
