use bevy::prelude::*;

use almanach::{Almanach, AlmanachAppExt, WallInfo};
use game_core::prelude::{EdgeSide, EdgeSides, GridArea, GridImprint, GridPosition, MapObject, MapObjectTooltipRequest, SSS, VolumeLevel};
use grids::placement::{PlacementModes, PlacementValidity, PlaceRequest, RemoveRequest};
use hud::prelude::BuilderSideMenuItemTooltip;
use indexes::{
    obstacles::GridStructure,
    placement::{annotate_not_placeable_ground, GridObjectPlacer, GridPlacerSession, GridsCollectionParam, validator_placeable_ground},
    prelude::ObstacleGrid,
};
use logging::prelude::*;
use map_objects::{
    prelude::{Wall, VolumeShape},
    wall_style::{WallStyleKey, WallStyles},
};
use persistence::{prelude::*, rusqlite};
use states::prelude::MapLoadingStage;

use crate::common::CellClearing;

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
impl From<WallStyleKey> for WallStyleSource {
    fn from(key: WallStyleKey) -> Self {
        Self::Key(key)
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
    pub shape: VolumeShape,
}
impl BuilderWall {
    pub fn almanach_info() -> WallInfo {
        WallInfo {
            name: "Wall".to_string(),
            description: "Blocks ground movement and shapes the paths wisps take.".to_string(),
            grid_imprint: WALL_GRID_IMPRINT,
            validate: validator_placeable_ground,
            annotate: annotate_not_placeable_ground,
            placement: PlacementModes::on_press(),
        }
    }

    pub fn new(grid_position: GridPosition, style: impl Into<WallStyleSource>) -> Self {
        Self { grid_position, style: style.into(), shape: VolumeShape::default() }
    }

    pub fn with_shape(mut self, shape: VolumeShape) -> Self {
        self.shape = shape;
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
                Wall,
                builder.shape,
                style,
            ));
    }
}

#[log_tags(Tag::GameSave)]
fn collect_walls(
    walls: Query<(Entity, &VolumeShape, &GridArea, &WallStyleKey), With<Wall>>,
    styles: Res<WallStyles>,
    mut save: SaveWriter,
) {
    if walls.is_empty() { return; }

    #[debug_dev("Saving {} walls", rows.len())]
    let rows: Vec<(u32, GridPosition, String, VolumeShape)> = walls
        .iter()
        .map(|(entity, shape, area, key)| {
            (
                entity.index_u32(),
                area.origin,
                // Save an empty name for an invalid key; loading uses the default style and logs a warning.
                styles.name_of(*key).unwrap_or_default().to_string(),
                *shape,
            )
        })
        .collect();
    save.submit(move |ctx| {
        for (id, grid_position, style, shape) in rows {
            ctx.register_entity(id)?;
            ctx.tx.prepare_cached("INSERT OR REPLACE INTO walls (id, style) VALUES (?1, ?2)")?
                .execute(rusqlite::params![id, style])?;
            match shape {
                VolumeShape::Ramp { high_side } => {
                    ctx.tx.prepare_cached("INSERT INTO wall_ramps (id, high_side) VALUES (?1, ?2)")?
                        .execute(rusqlite::params![id, high_side.as_ref()])?;
                }
                VolumeShape::Flat { openings } => {
                    for side in openings.iter() {
                        ctx.tx.prepare_cached("INSERT INTO wall_openings (id, side) VALUES (?1, ?2)")?
                            .execute(rusqlite::params![id, side.as_ref()])?;
                    }
                }
            }
            ctx.save_grid_position(id, grid_position)?;
        }
        Ok(())
    });
}

fn load_walls(ctx: &mut LoadContext) -> LoadResult {
    let query = "SELECT walls.id, walls.style, wall_ramps.high_side FROM walls LEFT JOIN wall_ramps ON wall_ramps.id = walls.id";
    ctx.for_each_entity(query, |ctx, old_id, entity, row| {
        let style: String = row.get(1)?;
        // Load openings only for flat walls; ramps cannot have tunnels.
        let shape = match row.get_parsed_optional::<EdgeSide>(2)? {
            Some(high_side) => VolumeShape::Ramp { high_side },
            None => {
                let mut statement = ctx.conn.prepare_cached("SELECT side FROM wall_openings WHERE id = ?1")?;
                let mut opening_rows = statement.query([old_id])?;
                let mut openings = EdgeSides::NONE;
                while let Some(opening_row) = opening_rows.next()? {
                    openings = openings.with(opening_row.get_parsed(0)?);
                }
                VolumeShape::Flat { openings }
            }
        };
        let grid_position = ctx.grid_position(old_id)?;
        ctx.insert(entity, BuilderWall::new(grid_position, style).with_shape(shape));
        Ok(())
    })
}

#[log_tags(Tag::MapObjects)]
fn on_insert_wall_imprint_on_obstacle_grid(
    trigger: On<Insert<Wall>>,
    mut obstacle_grid: ResMut<ObstacleGrid>,
    walls: Query<(&VolumeShape, &GridArea, &WallStyleKey), With<Wall>>,
) {
    let entity = trigger.entity;
    #[warn_dev("Wall {entity} has no shape, area or style; not imprinted on the obstacle grid")]
    let Ok((shape, area, style)) = walls.get(entity) else { return; };
    obstacle_grid.imprint_wall(*area, entity, *style, *shape);
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
    placer: Single<&GridArea, With<GridObjectPlacer>>,
    placer_session: Single<(&WallStyleKey, &VolumeShape), With<GridPlacerSession>>,
) {
    let PlaceRequest(MapObject::Wall) = *trigger else { return };
    let area = placer.into_inner();
    let (style, shape) = placer_session.into_inner();
    let validity = (almanach.walls.validate)(MapObject::Wall, *area, &grids);
    if validity == PlacementValidity::Invalid { return; }
    #[debug_player("Wall placed at {}", area.origin)]
    commands.spawn(BuilderWall::new(area.origin, *style).with_shape(*shape));
    grids.reserved_coords.reserve(*area);
}

#[log_tags(Tag::MapObjects)]
fn on_wall_remove_request_do_so(
    trigger: On<RemoveRequest>,
    mut commands: Commands,
    mut cell_clearing: CellClearing,
    obstacle_grid: Res<ObstacleGrid>,
    placer: Single<&GridArea, With<GridObjectPlacer>>,
) {
    let RemoveRequest(MapObject::Wall) = *trigger else { return };
    let coords = placer.origin.coords;
    #[debug_player("Wall removed at {coords}")]
    if let Some(GridStructure::Wall { slot, .. }) = obstacle_grid.get(coords).map(|cell| cell.elevation) {
        commands.entity(obstacle_grid.entry(slot).entity).despawn();
        // Clear the removed wall's upper floor; tunnel occupants still have ground beneath them.
        cell_clearing.clear_from(coords, VolumeLevel::OnElevation);
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
