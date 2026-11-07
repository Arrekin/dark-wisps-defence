use bevy::prelude::*;

use almanach::{Almanach, AlmanachAppExt, WallInfo};
use game_core::prelude::{EdgeSide, GridArea, GridImprint, GridPosition, MapObject, MapObjectTooltipRequest, SSS, VolumeLevel};
use grids::placement::{PlacementDirection, PlacementModes, PlacementStyle, PlacementValidity, PlaceRequest, RemoveRequest};
use hud::prelude::BuilderSideMenuItemTooltip;
use indexes::{
    floor_cover::FloorCoverGrid,
    obstacles::GridStructure,
    placement::{annotate_non_empty, GridObjectPlacer, GridsCollectionParam, validator_all_empty},
    prelude::ObstacleGrid,
    wisps::WispsGrid,
};
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
            .add_observer(on_insert_wall_imprint_on_obstacle_grid)
            .add_observer(on_discard_wall_deprint_from_obstacle_grid)
            .add_observer(on_wall_place_request_do_so)
            .add_observer(on_wall_remove_request_do_so)
            .add_observer(on_map_object_tooltip_request_spawn_wall_tooltip)
            .register_walls(BuilderWall::almanach_info());
    }
}

const WALL_GRID_IMPRINT: GridImprint = GridImprint::square(1);

/// Wall style supplied as a table key by placement or as a saved name by loading.
/// Names resolve against the current style table when the wall spawns.
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
    pub grid_position: GridPosition,
    pub style: WallStyleSource,
    /// The ramp's high side, or `None` for a flat wall.
    pub ramp_high_side: Option<EdgeSide>,
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

    pub fn new(grid_position: GridPosition, style: impl Into<WallStyleSource>) -> Self {
        Self { grid_position, style: style.into(), ramp_high_side: None }
    }

    pub fn with_ramp_high_side(mut self, ramp_high_side: Option<EdgeSide>) -> Self {
        self.ramp_high_side = ramp_high_side;
        self
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
                Wall { high_side: builder.ramp_high_side },
                style,
            ));
    }
}

#[log_tags(Tag::GameSave)]
fn collect_walls(
    walls: Query<(Entity, &Wall, &GridArea, &WallStyleKey)>,
    styles: Res<WallStyles>,
    mut save: SaveWriter,
) {
    if walls.is_empty() { return; }

    #[debug_dev("Saving {} walls", rows.len())]
    let rows: Vec<(u32, GridPosition, String, Option<EdgeSide>)> = walls
        .iter()
        .map(|(entity, wall, area, key)| {
            (
                entity.index_u32(),
                area.origin,
                // Save an empty name for an invalid key; loading uses the default style and logs a warning.
                styles.name_of(*key).unwrap_or_default().to_string(),
                wall.high_side,
            )
        })
        .collect();
    save.submit(move |ctx| {
        for (id, grid_position, style, high_side) in rows {
            ctx.register_entity(id)?;
            let high_side: Option<&str> = high_side.as_ref().map(|side| side.as_ref());
            ctx.tx.execute(
                "INSERT OR REPLACE INTO walls (id, style, high_side) VALUES (?1, ?2, ?3)",
                rusqlite::params![id, style, high_side],
            )?;
            ctx.save_grid_position(id, grid_position)?;
        }
        Ok(())
    });
}

fn load_walls(ctx: &mut LoadContext) -> LoadResult {
    ctx.for_each_entity("SELECT id, style, high_side FROM walls", |ctx, old_id, entity, row| {
        let style: String = row.get(1)?;
        let high_side = row.get_parsed_optional::<EdgeSide>(2)?;
        let grid_position = ctx.grid_position(old_id)?;
        ctx.insert(entity, BuilderWall::new(grid_position, style).with_ramp_high_side(high_side));
        Ok(())
    })
}

#[log_tags(Tag::MapObjects)]
fn on_insert_wall_imprint_on_obstacle_grid(
    trigger: On<Insert<Wall>>,
    mut obstacle_grid: ResMut<ObstacleGrid>,
    walls: Query<(&Wall, &GridArea, &WallStyleKey)>,
) {
    let entity = trigger.entity;
    #[warn_dev("Wall {entity} has no area or style; not imprinted on the obstacle grid")]
    let Ok((wall, area, style)) = walls.get(entity) else { return; };
    obstacle_grid.imprint_wall(*area, entity, *style, wall.high_side);
}

fn on_discard_wall_deprint_from_obstacle_grid(
    trigger: On<Discard<Wall>>,
    mut obstacle_grid: ResMut<ObstacleGrid>,
    walls: Query<&GridArea, With<Wall>>,
) {
    let Ok(area) = walls.get(trigger.entity) else { return; };
    obstacle_grid.deprint(*area);
}

#[log_tags(Tag::MapObjects)]
fn on_wall_place_request_do_so(
    trigger: On<PlaceRequest>,
    mut commands: Commands,
    almanach: Res<Almanach>,
    mut grids: GridsCollectionParam,
    placer: Single<(&GridArea, &PlacementStyle, &PlacementDirection), With<GridObjectPlacer>>,
) {
    let PlaceRequest(MapObject::Wall) = *trigger else { return };
    let (area, placement_style, placement_direction) = placer.into_inner();
    let validity = (almanach.walls.validate)(MapObject::Wall, *area, &grids);
    if validity == PlacementValidity::Invalid { return; }
    #[debug_player("Wall placed at {}", area.origin)]
    commands.spawn(BuilderWall::new(area.origin, *placement_style).with_ramp_high_side(placement_direction.0));
    grids.reserved_coords.reserve(*area);
}

#[log_tags(Tag::MapObjects)]
fn on_wall_remove_request_do_so(
    trigger: On<RemoveRequest>,
    mut commands: Commands,
    mut wisps_grid: ResMut<WispsGrid>,
    floor_cover_grid: Res<FloorCoverGrid>,
    obstacle_grid: Res<ObstacleGrid>,
    placer: Single<&GridArea, With<GridObjectPlacer>>,
) {
    let RemoveRequest(MapObject::Wall) = *trigger else { return };
    let coords = placer.origin.coords;
    #[debug_player("Wall removed at {coords}")]
    if let Some(GridStructure::Wall { slot, .. }) = obstacle_grid.get(coords).map(|cell| cell.elevation) {
        commands.entity(obstacle_grid.entry(slot).entity).despawn();

        // Remove wisps whose supporting wall is being removed.
        let top = coords.at(VolumeLevel::OnElevation);
        for wisp in wisps_grid.wisps_at(top).to_vec() {
            wisps_grid.wisp_remove(top, wisp);
            commands.entity(wisp).despawn();
        }

        // Remove floor covers lying on the wall.
        for entry in floor_cover_grid.entries_at(coords).iter().filter(|entry| entry.level == top.level.floor_below()) {
            commands.entity(entry.entity).despawn();
        }
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
