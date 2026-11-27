use bevy::{ecs::system::SystemParam, prelude::*};

use almanach::{Almanach, AlmanachAppExt, WallInfo};
use game_core::prelude::{AltitudeLevel, EdgeSide, EdgeSides, GridArea, GridCoords, GridImprint, GridPosition, MapObject, MapObjectTooltipRequest, SSS};
use grids::placement::{
    BeginPlacing, PlaceRequest, PlaceResult, PlacementFeedback, PlacementFeedbackSystems, PlacementModes, PlacementObject, PlacementSession,
    PlacementValidity, RemoveRequest, RemoveResult, ReplaceExisting, ReservedCoords, UiPlacementSession,
};
use hud::prelude::BuilderSideMenuItemTooltip;
use indexes::{floor_cover::FloorCoverGrid, obstacles::GridStructure, prelude::{ObstacleCell, ObstacleGrid}, wisps::WispsGrid};
use logging::prelude::*;
use map_objects::{
    prelude::{StructureShape, Wall},
    wall_style::{WallStyleKey, WallStyles},
};
use persistence::{prelude::*, rusqlite};
use states::prelude::MapLoadingStage;

use crate::wall_editor_ui::GridPlacerUiForWall;

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
            .add_observer(on_begin_placing_set_up_wall_placement_session)
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
    grid_position: GridPosition,
    style: WallStyleSource,
    shape: StructureShape,
}
impl BuilderWall {
    pub(crate) fn almanach_info() -> WallInfo {
        WallInfo {
            name: "Wall".to_string(),
            description: "Blocks ground movement and shapes the paths wisps take.".to_string(),
            grid_imprint: WALL_GRID_IMPRINT,
        }
    }

    pub(crate) fn new(grid_position: GridPosition, style: impl Into<WallStyleSource>) -> Self {
        Self { grid_position, style: style.into(), shape: StructureShape::default() }
    }

    pub(crate) fn with_shape(mut self, shape: StructureShape) -> Self {
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
    walls: Query<(Entity, &StructureShape, &GridArea, &WallStyleKey), With<Wall>>,
    styles: Res<WallStyles>,
    mut save: SaveWriter,
) {
    if walls.is_empty() { return; }

    #[debug_dev("Saving {} walls", rows.len())]
    let rows: Vec<(u32, GridPosition, String, StructureShape)> = walls
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
                StructureShape::Ramp { high_side } => {
                    ctx.tx.prepare_cached("INSERT INTO wall_ramps (id, high_side) VALUES (?1, ?2)")?
                        .execute(rusqlite::params![id, high_side.as_ref()])?;
                }
                StructureShape::Flat { hollowed_sides } => {
                    for side in hollowed_sides.iter() {
                        ctx.tx.prepare_cached("INSERT INTO wall_hollowed_sides (id, side) VALUES (?1, ?2)")?
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
        // Load hollowed sides only for flat walls; ramps have none.
        let shape = match row.get_parsed_optional::<EdgeSide>(2)? {
            Some(high_side) => StructureShape::Ramp { high_side },
            None => {
                let hollowed_sides = ctx.conn.prepare_cached("SELECT side FROM wall_hollowed_sides WHERE id = ?1")?
                    .query_and_then([old_id], |row| row.get_parsed(0))?
                    .collect::<LoadResult<EdgeSides>>()?;
                StructureShape::Flat { hollowed_sides }
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
    walls: Query<(&StructureShape, &GridArea, &WallStyleKey), With<Wall>>,
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

/// Initializes a wall placement session at `trigger.origin` with default style, default shape,
/// and drag placement, and spawns UI controls for [`UiPlacementSession`].
fn on_begin_placing_set_up_wall_placement_session(
    trigger: On<BeginPlacing>,
    mut commands: Commands,
    placement_sessions: Query<(&PlacementObject, Has<UiPlacementSession>)>,
) {
    let Ok((&PlacementObject(MapObject::Wall), is_ui_placement_session)) = placement_sessions.get(trigger.placement_session) else { return };
    commands.entity(trigger.placement_session).insert((
        GridArea::new(trigger.origin, WALL_GRID_IMPRINT),
        PlacementModes::on_press(),
        WallStyleKey::default(),
        StructureShape::default(),
    ));
    if is_ui_placement_session {
        commands.spawn(GridPlacerUiForWall);
    }
}

/// Validates, places, replaces, and removes walls.
#[derive(SystemParam)]
pub(crate) struct WallPlacementRules<'w, 's> {
    commands: Commands<'w, 's>,
    obstacle_grid: Res<'w, ObstacleGrid>,
    reserved_coords: ResMut<'w, ReservedCoords>,
    wisps_grid: ResMut<'w, WispsGrid>,
    floor_cover_grid: Res<'w, FloorCoverGrid>,
}
impl WallPlacementRules<'_, '_> {
    /// Requires unreserved, placeable ground (or existing walls when `replace` is true, reporting
    /// [`PlacementValidity::Replaces`]).
    fn feedback(&self, area: GridArea, replace: bool) -> PlacementFeedback {
        let replaceable = |coords| replace && self.wall_at(coords).is_some();
        PlacementFeedback::every_cell(area, &self.reserved_coords, |coords| {
            replaceable(coords) || self.obstacle_grid.get(coords).is_some_and(ObstacleCell::is_placeable_ground)
        }).replacing(area, replaceable)
    }

    fn wall_at(&self, coords: GridCoords) -> Option<Entity> {
        match self.obstacle_grid.get(coords)?.on_ground {
            GridStructure::Wall { slot, .. } => Some(self.obstacle_grid.entry(slot).entity),
            _ => None,
        }
    }

    #[log_tags(Tag::MapObjects)]
    fn place(&mut self, area: GridArea, replace: bool, style: WallStyleKey, shape: StructureShape) -> Result<(), PlaceResult> {
        let validity = self.feedback(area, replace).validity;
        if validity == PlacementValidity::Invalid { return Err(PlaceResult::Invalid); }
        if validity == PlacementValidity::Replaces {
            for coords in area {
                if let Some(wall) = self.wall_at(coords) {
                    self.commands.entity(wall).despawn();
                }
            }
        }
        // Solid and ramp walls fill `OnGround`, displacing occupants at `OnGround` and above.
        if !shape.is_hollow() {
            for coords in area {
                self.wisps_grid.despawn_from_level_upwards(&mut self.commands, coords, AltitudeLevel::OnGround);
                self.floor_cover_grid.despawn_from_level_upwards(&mut self.commands, coords, AltitudeLevel::OnGround);
            }
        }
        #[debug_player("Wall placed at {}", area.origin)]
        self.commands.spawn(BuilderWall::new(area.origin, style).with_shape(shape));
        self.reserved_coords.reserve(area);
        Ok(())
    }

    #[log_tags(Tag::MapObjects)]
    fn remove(&mut self, coords: GridCoords) -> Result<(), RemoveResult> {
        let wall = self.wall_at(coords).ok_or(RemoveResult::NothingToRemove)?;
        #[debug_player("Wall removed at {coords}")]
        self.commands.entity(wall).despawn();
        // Removing the wall removes the `OnElevation` floor above it.
        self.wisps_grid.despawn_from_level_upwards(&mut self.commands, coords, AltitudeLevel::OnElevation);
        self.floor_cover_grid.despawn_from_level_upwards(&mut self.commands, coords, AltitudeLevel::OnElevation);
        Ok(())
    }

    fn update_feedback(
        mut rules: WallPlacementRules,
        ui_placement_session: Single<(Entity, &PlacementObject, &GridArea, Has<ReplaceExisting>), With<UiPlacementSession>>,
    ) {
        let (ui_placement_session_entity, &PlacementObject(MapObject::Wall), &area, replace) = *ui_placement_session else { return };
        let feedback = rules.feedback(area, replace);
        rules.commands.entity(ui_placement_session_entity).insert_if_neq(feedback);
    }

    fn on_place_request_do_so(
        trigger: On<PlaceRequest>,
        mut rules: WallPlacementRules,
        placement_sessions: Query<(&PlacementObject, Option<(&GridArea, &WallStyleKey, &StructureShape)>, Has<ReplaceExisting>), With<PlacementSession>>,
    ) {
        let Ok((&PlacementObject(MapObject::Wall), settings, replace)) = placement_sessions.get(trigger.placement_session) else { return };
        let outcome = settings.ok_or(PlaceResult::InvalidSession).and_then(|(&area, &style, &shape)| rules.place(area, replace, style, shape));
        trigger.report(&mut rules.commands, outcome);
    }

    fn on_remove_request_do_so(
        trigger: On<RemoveRequest>,
        mut rules: WallPlacementRules,
        placement_sessions: Query<(&PlacementObject, Option<&GridArea>), With<PlacementSession>>,
    ) {
        let Ok((&PlacementObject(MapObject::Wall), area)) = placement_sessions.get(trigger.placement_session) else { return };
        let outcome = area.ok_or(RemoveResult::InvalidSession).and_then(|area| rules.remove(area.origin.coords));
        trigger.report(&mut rules.commands, outcome);
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
