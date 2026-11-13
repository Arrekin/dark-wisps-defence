use bevy::{ecs::system::SystemParam, prelude::*};

use almanach::{Almanach, AlmanachAppExt, WallInfo};
use game_core::prelude::{EdgeSide, EdgeSides, GridArea, GridCoords, GridImprint, GridPosition, MapObject, MapObjectTooltipRequest, SSS, VolumeLevel};
use grids::placement::{PlacementFeedback, PlacementObject, PlacementSession, PlacementValidity, PlacementFeedbackSystems, PlaceRequest, RemoveRequest, ReplaceExisting, ReservedCoords};
use hud::prelude::BuilderSideMenuItemTooltip;
use indexes::{obstacles::GridStructure, prelude::{ObstacleCell, ObstacleGrid}};
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
            .add_systems(Update, WallPlacementRules::update_feedback.in_set(PlacementFeedbackSystems))
            .add_systems(CollectSave, collect_walls)
            .register_loader(MapLoadingStage::SpawnMapElements, "walls", load_walls)
            .add_observer(BuilderWall::on_builder_add_spawn_wall)
            .add_observer(on_insert_wall_imprint_on_obstacle_grid)
            .add_observer(on_discard_wall_deprint_from_obstacle_grid)
            .add_observer(WallPlacementRules::on_place_request_do_so)
            .add_observer(WallPlacementRules::on_remove_request_do_so)
            .add_observer(on_map_object_tooltip_request_spawn_wall_tooltip)
            .register_walls(BuilderWall::almanach_info());
    }
}

pub(crate) const WALL_GRID_IMPRINT: GridImprint = GridImprint::square(1);

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

/// Validates, places, replaces, and removes walls at the session's target area.
#[derive(SystemParam)]
pub(crate) struct WallPlacementRules<'w, 's> {
    commands: Commands<'w, 's>,
    obstacle_grid: Res<'w, ObstacleGrid>,
    reserved_coords: ResMut<'w, ReservedCoords>,
    placement_session: Single<'w, 's, (&'static GridArea, Has<ReplaceExisting>), With<PlacementSession>>,
}
impl WallPlacementRules<'_, '_> {
    /// Requires unreserved, placeable ground. With `ReplaceExisting`, also accepts existing walls
    /// and reports `Replaces` if the area contains any.
    fn feedback(&self) -> PlacementFeedback {
        let (&area, replace) = *self.placement_session;
        let replaceable = |coords| replace && self.wall_at(coords).is_some();
        PlacementFeedback::every_cell(area, &self.reserved_coords, |coords| {
            replaceable(coords) || self.obstacle_grid.get(coords).is_some_and(ObstacleCell::is_placeable_ground)
        }).replacing(area, replaceable)
    }

    fn wall_at(&self, coords: GridCoords) -> Option<Entity> {
        match self.obstacle_grid.get(coords)?.elevation {
            GridStructure::Wall { slot, .. } => Some(self.obstacle_grid.entry(slot).entity),
            _ => None,
        }
    }

    fn update_feedback(
        mut rules: WallPlacementRules,
        placement_session: Single<(Entity, &PlacementObject), With<PlacementSession>>,
    ) {
        let (placement_session, object) = *placement_session;
        if object.0 != MapObject::Wall { return; }
        let feedback = rules.feedback();
        rules.commands.entity(placement_session).insert_if_neq(feedback);
    }

    #[log_tags(Tag::MapObjects)]
    fn on_place_request_do_so(
        trigger: On<PlaceRequest>,
        mut rules: WallPlacementRules,
        wall_settings: Single<(&WallStyleKey, &VolumeShape), With<PlacementSession>>,
    ) {
        let PlaceRequest(MapObject::Wall) = *trigger else { return };
        let (&area, _) = *rules.placement_session;
        let (&style, &shape) = *wall_settings;
        let validity = rules.feedback().validity;
        if validity == PlacementValidity::Invalid { return; }
        if validity == PlacementValidity::Replaces {
            for coords in area {
                if let Some(wall) = rules.wall_at(coords) {
                    rules.commands.entity(wall).despawn();
                }
            }
        }
        #[debug_player("Wall placed at {}", area.origin)]
        rules.commands.spawn(BuilderWall::new(area.origin, style).with_shape(shape));
        rules.reserved_coords.reserve(area);
    }

    #[log_tags(Tag::MapObjects)]
    fn on_remove_request_do_so(
        trigger: On<RemoveRequest>,
        mut rules: WallPlacementRules,
        mut cell_clearing: CellClearing,
    ) {
        let RemoveRequest(MapObject::Wall) = *trigger else { return };
        let (area, _) = *rules.placement_session;
        let coords = area.origin.coords;
        #[debug_player("Wall removed at {coords}")]
        if let Some(wall) = rules.wall_at(coords) {
            rules.commands.entity(wall).despawn();
            // Remove occupants of the wall's upper floor; lower levels keep their supporting floors.
            cell_clearing.clear_from(coords, VolumeLevel::OnElevation);
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
