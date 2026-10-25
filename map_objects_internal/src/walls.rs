use bevy::prelude::*;

use almanach::{Almanach, AlmanachAppExt, WallInfo};
use game_core::prelude::{GridArea, GridCoords, GridImprint, MapObject, MapObjectTooltipRequest, SSS};
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
    prelude::Wall,
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
            .add_observer(on_map_object_tooltip_request_spawn_wall_tooltip)
            .register_walls(BuilderWall::almanach_info());
    }
}

const WALL_GRID_IMPRINT: GridImprint = GridImprint::square(1);

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
        }
    }

    pub fn new(grid_position: GridCoords, style: impl Into<WallStyleSource>) -> Self {
        Self { grid_position, style: style.into() }
    }

    #[log_tags(Tag::GameLoad)]
    fn on_builder_add_spawn_wall(
        trigger: On<Add<BuilderWall>>,
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
                GridArea::new(builder.grid_position, WALL_GRID_IMPRINT),
                Wall,
                style,
            ));
    }
}

#[log_tags(Tag::GameSave)]
fn collect_walls(
    walls: Query<(Entity, &GridArea, &WallStyleKey), With<Wall>>,
    styles: Res<WallStyles>,
    mut save: SaveWriter,
) {
    if walls.is_empty() { return; }

    #[debug_dev("Saving {} walls", rows.len())]
    let rows: Vec<(u32, GridCoords, String)> = walls
        .iter()
        .map(|(entity, area, key)| {
            (
                entity.index_u32(),
                area.origin,
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
    placer: Single<(&GridArea, &PlacementStyle), With<GridObjectPlacer>>,
) {
    let PlaceRequest(MapObject::Wall) = *trigger else { return };
    let (area, placement_style) = placer.into_inner();
    let validity = (almanach.walls.validate)(MapObject::Wall, *area, &grids);
    if validity == PlacementValidity::Invalid { return; }
    #[debug_dev("Wall placed at {}", area.origin)]
    commands.spawn(BuilderWall::new(area.origin, *placement_style));
    grids.reserved_coords.reserve(*area);
}

#[log_tags(Tag::MapObjects)]
fn on_wall_remove_request_do_so(
    trigger: On<RemoveRequest>,
    mut commands: Commands,
    grids: GridsCollectionParam,
    placer: Single<&GridArea, With<GridObjectPlacer>>,
) {
    let RemoveRequest(MapObject::Wall) = *trigger else { return };
    let coords = placer.origin;
    #[debug_dev("Wall removed at {coords}")]
    if let Some(&GridStructureType::Wall(entity)) = grids.obstacle_grid.get(coords).map(|cell| &cell.structure) {
        commands.entity(entity).despawn();
    }
}

fn on_map_object_tooltip_request_spawn_wall_tooltip(
    trigger: On<MapObjectTooltipRequest>,
    mut commands: Commands,
    almanach: Res<Almanach>,
) {
    let MapObjectTooltipRequest { entity, object: MapObject::Wall } = *trigger else { return };

    let info = &almanach.walls;
    commands.spawn(
        BuilderSideMenuItemTooltip::new(entity)
            .with_name(info.name.clone())
            .with_description(info.description.clone())
            .with_fact(info.grid_imprint.label()),
    );
}
