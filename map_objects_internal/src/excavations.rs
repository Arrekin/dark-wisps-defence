//! # Excavations
//!
//! Excavations store pits, ramps and flat shapes directly in the obstacle grid's `Underground` level.
//! Placement digs an [`ExcavationShape`] into solid earth. With [`ReplaceExisting`], it can also
//! reshape an existing excavation. Removal fills the excavation with solid earth.
//!
//! Flat shapes keep the ground floor, so they can be dug beneath walls and buildings. Digging a
//! pit or ramp removes that floor and clears wisps and floor covers at every level of the cell.
//! Filling an excavation also clears every level. Each edit explicitly requests an emissions
//! rebuild because emissions do not track obstacle grid versions.
//!
//! Save collectors read excavations from the grid; loaders restore them during `LoadResources`.
//! Pits have their own table; shaped excavations store ramp directions and hollowed sides in
//! child tables.

use bevy::{ecs::system::SystemParam, prelude::*};

use almanach::{Almanach, AlmanachAppExt, ExcavationInfo};
use game_core::{prelude::{AltitudeLevel, EdgeSide, EdgeSides, GridArea, GridCoords, GridImprint, MapObject, MapObjectTooltipRequest}, request_flag::RequestFlag};
use grids::placement::{
    BeginPlacing, PlaceRequest, PlaceResult, PlacementFeedback, PlacementFeedbackSystems, PlacementModes, PlacementObject, PlacementSession,
    PlacementValidity, RemoveRequest, RemoveResult, ReplaceExisting, ReservedCoords, UiPlacementSession,
};
use hud::prelude::BuilderSideMenuItemTooltip;
use indexes::{emissions::EmissionsRecalculation, floor_cover::FloorCoverGrid, obstacles::GridStructure, prelude::{ObstacleCell, ObstacleGrid}, wisps::WispsGrid};
use logging::prelude::*;
use map_objects::prelude::{ExcavationShape, StructureShape};
use persistence::{prelude::*, rusqlite};
use states::prelude::MapLoadingStage;

use crate::excavation_editor_ui::GridPlacerUiForExcavation;

pub(crate) struct ExcavationPlugin;
impl Plugin for ExcavationPlugin {
    fn build(&self, app: &mut App) {
        app
            .add_systems(Update, ExcavationPlacementRules::update_feedback.in_set(PlacementFeedbackSystems))
            .add_observer(on_begin_placing_set_up_excavation_placement_session)
            .add_observer(ExcavationPlacementRules::on_place_request_do_so)
            .add_observer(ExcavationPlacementRules::on_remove_request_do_so)
            .add_observer(on_map_object_tooltip_request_spawn_excavation_tooltip)
            .add_systems(CollectSave, collect_excavations)
            .register_loader(MapLoadingStage::LoadResources, "excavation_pits", load_excavation_pits)
            .register_loader(MapLoadingStage::LoadResources, "excavation_shapes", load_excavation_shapes)
            .register_excavations(almanach_info());
    }
}

pub(crate) const EXCAVATION_GRID_IMPRINT: GridImprint = GridImprint::square(1);

fn almanach_info() -> ExcavationInfo {
    ExcavationInfo {
        name: "Excavation".to_string(),
        description: "Opens the ground below: a pit, a ramp down to a pit floor, or a tunnel under the ground. Right-click fills the cell back with earth.".to_string(),
        grid_imprint: EXCAVATION_GRID_IMPRINT,
    }
}

/// Initializes an excavation placement session at `trigger.origin` (defaulting to
/// [`ExcavationShape::Pit`] and drag placement) and spawns UI controls for [`UiPlacementSession`].
fn on_begin_placing_set_up_excavation_placement_session(
    trigger: On<BeginPlacing>,
    mut commands: Commands,
    placement_sessions: Query<(&PlacementObject, Has<UiPlacementSession>)>,
) {
    let Ok((&PlacementObject(MapObject::Excavation), is_ui_placement_session)) = placement_sessions.get(trigger.placement_session) else { return };
    commands.entity(trigger.placement_session).insert((
        GridArea::new(trigger.origin, EXCAVATION_GRID_IMPRINT),
        PlacementModes::on_press(),
        ExcavationShape::Pit,
    ));
    if is_ui_placement_session {
        commands.spawn(GridPlacerUiForExcavation);
    }
}

/// Validates and applies excavation edits, clearing displaced occupants and requesting emissions recalculation.
#[derive(SystemParam)]
pub(crate) struct ExcavationPlacementRules<'w, 's> {
    commands: Commands<'w, 's>,
    obstacle_grid: ResMut<'w, ObstacleGrid>,
    reserved_coords: Res<'w, ReservedCoords>,
    wisps_grid: ResMut<'w, WispsGrid>,
    floor_cover_grid: Res<'w, FloorCoverGrid>,
    emissions_recalculation: ResMut<'w, RequestFlag<EmissionsRecalculation>>,
}
impl ExcavationPlacementRules<'_, '_> {
    /// Requires unreserved solid earth (or an existing excavation when `replace` is true).
    /// Pits and ramps also require the cell to have no wall or building above it.
    /// Reports `Replaces` if the area is valid and contains an excavation to replace.
    fn feedback(&self, area: GridArea, shape: ExcavationShape, replace: bool) -> PlacementFeedback {
        let replaceable = |cell: &ObstacleCell| replace && cell.excavation().is_some();
        PlacementFeedback::every_cell(area, &self.reserved_coords, |coords| {
            self.obstacle_grid.get(coords).is_some_and(|cell| {
                let can_be_dug = cell.underground == GridStructure::Solid || replaceable(cell);
                can_be_dug && (shape.keeps_ground_floor() || !cell.has_structure())
            })
        }).replacing(area, |coords| self.obstacle_grid.get(coords).is_some_and(replaceable))
    }

    fn holds_excavation(&self, coords: GridCoords) -> bool {
        self.obstacle_grid.get(coords).is_some_and(|cell| cell.excavation().is_some())
    }

    /// Digs `shape` at the area's origin and requests an emissions rebuild.
    #[log_tags(Tag::MapObjects)]
    fn place(&mut self, area: GridArea, shape: ExcavationShape, replace: bool) -> Result<(), PlaceResult> {
        if self.feedback(area, shape, replace).validity == PlacementValidity::Invalid { return Err(PlaceResult::Invalid); }
        let coords = area.origin.coords;
        #[debug_player("Excavated {coords} into {shape:?}")]
        self.obstacle_grid.excavate(coords, shape);
        // Pits and ramps remove the ground floor, invalidating occupants at all levels.
        if !shape.keeps_ground_floor() {
            self.wisps_grid.despawn_from_level_upwards(&mut self.commands, coords, AltitudeLevel::Underground);
            self.floor_cover_grid.despawn_from_level_upwards(&mut self.commands, coords, AltitudeLevel::Underground);
        }
        self.emissions_recalculation.request();
        Ok(())
    }

    /// Fills the excavation at `coords` and requests an emissions rebuild.
    #[log_tags(Tag::MapObjects)]
    fn remove(&mut self, coords: GridCoords) -> Result<(), RemoveResult> {
        if !self.holds_excavation(coords) { return Err(RemoveResult::NothingToRemove); }
        #[debug_player("Excavation filled at {coords}")]
        self.obstacle_grid.fill_excavation(coords);
        // Filling removes the underground space and any occupants in the cell.
        self.wisps_grid.despawn_from_level_upwards(&mut self.commands, coords, AltitudeLevel::Underground);
        self.floor_cover_grid.despawn_from_level_upwards(&mut self.commands, coords, AltitudeLevel::Underground);
        self.emissions_recalculation.request();
        Ok(())
    }

    fn update_feedback(
        mut rules: ExcavationPlacementRules,
        ui_placement_session: Single<(Entity, &PlacementObject, &GridArea, &ExcavationShape, Has<ReplaceExisting>), With<UiPlacementSession>>,
    ) {
        let (ui_placement_session_entity, &PlacementObject(MapObject::Excavation), &area, &shape, replace) = *ui_placement_session else { return };
        let feedback = rules.feedback(area, shape, replace);
        rules.commands.entity(ui_placement_session_entity).insert_if_neq(feedback);
    }

    fn on_place_request_do_so(
        trigger: On<PlaceRequest>,
        mut rules: ExcavationPlacementRules,
        placement_sessions: Query<(&PlacementObject, Option<(&GridArea, &ExcavationShape)>, Has<ReplaceExisting>), With<PlacementSession>>,
    ) {
        let Ok((&PlacementObject(MapObject::Excavation), settings, replace)) = placement_sessions.get(trigger.placement_session) else { return };
        let outcome = settings.ok_or(PlaceResult::InvalidSession).and_then(|(&area, &shape)| rules.place(area, shape, replace));
        trigger.report(&mut rules.commands, outcome);
    }

    fn on_remove_request_do_so(
        trigger: On<RemoveRequest>,
        mut rules: ExcavationPlacementRules,
        placement_sessions: Query<(&PlacementObject, Option<&GridArea>), With<PlacementSession>>,
    ) {
        let Ok((&PlacementObject(MapObject::Excavation), area)) = placement_sessions.get(trigger.placement_session) else { return };
        let outcome = area.ok_or(RemoveResult::InvalidSession).and_then(|area| rules.remove(area.origin.coords));
        trigger.report(&mut rules.commands, outcome);
    }
}

#[log_tags(Tag::GameSave)]
fn collect_excavations(
    obstacle_grid: Res<ObstacleGrid>,
    mut save: SaveWriter,
) {
    let rows: Vec<(GridCoords, ExcavationShape)> = obstacle_grid.bounds.iter()
        .filter_map(|coords| obstacle_grid.cell(coords).excavation().map(|shape| (coords, shape)))
        .collect();
    if rows.is_empty() { return; }

    debug_dev!("Saving {} excavated cells", rows.len());
    save.submit(move |ctx| {
        for (coords, shape) in rows {
            let GridCoords { x, y } = coords;
            match shape {
                ExcavationShape::Pit => {
                    ctx.tx.prepare_cached("INSERT INTO excavation_pits (x, y) VALUES (?1, ?2)")?
                        .execute(rusqlite::params![x, y])?;
                }
                ExcavationShape::Shaped(shape) => {
                    ctx.tx.prepare_cached("INSERT INTO excavation_shapes (x, y) VALUES (?1, ?2)")?
                        .execute(rusqlite::params![x, y])?;
                    match shape {
                        StructureShape::Ramp { high_side } => {
                            ctx.tx.prepare_cached("INSERT INTO excavation_ramps (x, y, high_side) VALUES (?1, ?2, ?3)")?
                                .execute(rusqlite::params![x, y, high_side.as_ref()])?;
                        }
                        StructureShape::Flat { hollowed_sides } => {
                            for side in hollowed_sides.iter() {
                                ctx.tx.prepare_cached("INSERT INTO excavation_hollowed_sides (x, y, side) VALUES (?1, ?2, ?3)")?
                                    .execute(rusqlite::params![x, y, side.as_ref()])?;
                            }
                        }
                    }
                }
            }
        }
        Ok(())
    });
}

fn load_excavation_pits(ctx: &mut LoadContext) -> LoadResult {
    ctx.for_each_row("SELECT x, y FROM excavation_pits", |ctx, row| {
        let coords = GridCoords { x: row.get(0)?, y: row.get(1)? };
        ctx.push(excavate_loaded_cell(coords, ExcavationShape::Pit));
        Ok(())
    })
}

fn load_excavation_shapes(ctx: &mut LoadContext) -> LoadResult {
    let query = "SELECT excavation_shapes.x, excavation_shapes.y, excavation_ramps.high_side FROM excavation_shapes \
        LEFT JOIN excavation_ramps ON excavation_ramps.x = excavation_shapes.x AND excavation_ramps.y = excavation_shapes.y";
    ctx.for_each_row(query, |ctx, row| {
        let coords = GridCoords { x: row.get(0)?, y: row.get(1)? };
        // Load hollowed sides only for flat shapes; ramps have none.
        let shape = match row.get_parsed_optional::<EdgeSide>(2)? {
            Some(high_side) => StructureShape::Ramp { high_side },
            None => {
                let hollowed_sides = ctx.conn.prepare_cached("SELECT side FROM excavation_hollowed_sides WHERE x = ?1 AND y = ?2")?
                    .query_and_then([coords.x, coords.y], |row| row.get_parsed(0))?
                    .collect::<LoadResult<EdgeSides>>()?;
                StructureShape::Flat { hollowed_sides }
            }
        };
        ctx.push(excavate_loaded_cell(coords, ExcavationShape::Shaped(shape)));
        Ok(())
    })
}

/// Restores a saved excavation and requests an emissions rebuild.
/// Logs and skips coordinates outside the map.
#[log_tags(Tag::GameLoad)]
fn excavate_loaded_cell(coords: GridCoords, shape: ExcavationShape) -> impl Command {
    move |world: &mut World| {
        let mut obstacle_grid = world.resource_mut::<ObstacleGrid>();
        #[warn_dev("Saved excavation at {coords} is outside the map; skipped")]
        if !obstacle_grid.bounds.contains(coords) { return; }
        obstacle_grid.excavate(coords, shape);
        world.resource_mut::<RequestFlag<EmissionsRecalculation>>().request();
    }
}

fn on_map_object_tooltip_request_spawn_excavation_tooltip(
    trigger: On<MapObjectTooltipRequest>,
    mut commands: Commands,
    almanach: Res<Almanach>,
) {
    let MapObjectTooltipRequest { entity, object: MapObject::Excavation } = *trigger else { return };

    let info = &almanach.excavations;
    commands.spawn(
        BuilderSideMenuItemTooltip::new(entity)
            .with_name(info.name.clone())
            .with_description(info.description.clone())
            .with_fact(info.grid_imprint.label()),
    );
}
