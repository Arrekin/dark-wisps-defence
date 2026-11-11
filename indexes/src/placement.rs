//! Placement state and callbacks that read spatial indexes.
//! Shared placement events and modes live in `grids::placement`.

use bevy::{ecs::system::SystemParam, prelude::*};

use game_core::prelude::{AutoGridSnap, DrawLevel, GridArea, GridCoords, GridImprint, MapObject, ZDepth};
use grids::placement::{CellHighlight, PlacementModes, PlacementValidity, ReservedCoords};

use crate::{energy_supply::EnergySupplyGrid, obstacles::{ObstacleCell, ObstacleGrid}, wisps::WispsGrid};

/// Grid resources used by placement validators and request handlers.
#[derive(SystemParam)]
pub struct GridsCollectionParam<'w> {
    pub obstacle_grid: Res<'w, ObstacleGrid>,
    pub energy_supply_grid: Res<'w, EnergySupplyGrid>,
    /// Mutable so handlers can reserve cells before a deferred spawn updates the obstacle grid.
    pub reserved_coords: ResMut<'w, ReservedCoords>,
    pub wisps_grid: Res<'w, WispsGrid>,
}

/// Checks whether the object can be placed in the supplied area.
pub type PlacementValidatorFn = fn(MapObject, GridArea, &GridsCollectionParam) -> PlacementValidity;

/// Produces cell highlights for the placement ghost using the validation result.
/// For example, an annotator can highlight blocked cells when placement is invalid.
pub type PlacementAnnotatorFn = fn(MapObject, GridArea, PlacementValidity, &GridsCollectionParam) -> Vec<(GridCoords, CellHighlight)>;

/// Imprint, input modes, and callbacks supplied by Almanach for one object type.
pub struct ObjectPlacementInfo {
    pub imprint: GridImprint,
    pub validate: PlacementValidatorFn,
    pub annotate: PlacementAnnotatorFn,
    pub placement: PlacementModes,
}

/// Active placement session data.
pub struct ActivePlacement {
    pub map_object: MapObject,
    pub placement_info: ObjectPlacementInfo,
}

#[derive(Component, Default)]
#[require(GridArea, ZDepth::GridPlacer, DrawLevel::Airborne, AutoGridSnap)]
pub struct GridObjectPlacer {
    pub active_placement: Option<ActivePlacement>,
}

impl GridObjectPlacer {
    pub fn map_object(&self) -> Option<MapObject> {
        self.active_placement.as_ref().map(|active_placement| active_placement.map_object)
    }
}

/// Marks the placer child that stores domain parameters, such as a wall's style and shape.
/// At the start of each session, the placer removes all components except this marker and `ChildOf`.
#[derive(Component, Default)]
pub struct GridPlacerSession;

#[derive(Resource, Default)]
pub struct GridObjectPlacerRequest(Option<MapObject>);
impl GridObjectPlacerRequest {
    pub fn is_set(&self) -> bool { self.0.is_some() }
    pub fn set(&mut self, request: MapObject) { self.0 = Some(request); }
    pub fn take(&mut self) -> Option<MapObject> { self.0.take() }

    /// Run condition that is true while a placement request is pending.
    pub fn is_requested(request: Res<Self>) -> bool {
        request.is_set()
    }
}

/// Accepts an area when every cell is in bounds, unreserved, and satisfies [`ObstacleCell::is_open_ground`].
pub fn validator_open_ground(
    _: MapObject,
    area: GridArea,
    grids: &GridsCollectionParam,
) -> PlacementValidity {
    validate_every_cell(area, grids, ObstacleCell::is_open_ground)
}

/// Accepts an area when every cell is in bounds, unreserved, and satisfies [`ObstacleCell::is_placeable_ground`].
pub fn validator_placeable_ground(
    _: MapObject,
    area: GridArea,
    grids: &GridsCollectionParam,
) -> PlacementValidity {
    validate_every_cell(area, grids, ObstacleCell::is_placeable_ground)
}

/// Accepts an area when every cell is in bounds, unreserved, and satisfies [`ObstacleCell::can_be_excavated`].
pub fn validator_excavatable(
    _: MapObject,
    area: GridArea,
    grids: &GridsCollectionParam,
) -> PlacementValidity {
    validate_every_cell(area, grids, ObstacleCell::can_be_excavated)
}

/// On invalid placement, highlights cells outside the map or failing [`ObstacleCell::is_open_ground`].
pub fn annotate_not_open_ground(
    _: MapObject,
    area: GridArea,
    validity: PlacementValidity,
    grids: &GridsCollectionParam,
) -> Vec<(GridCoords, CellHighlight)> {
    annotate_failing_cells(area, validity, grids, ObstacleCell::is_open_ground)
}

/// On invalid placement, highlights cells outside the map or failing [`ObstacleCell::is_placeable_ground`].
pub fn annotate_not_placeable_ground(
    _: MapObject,
    area: GridArea,
    validity: PlacementValidity,
    grids: &GridsCollectionParam,
) -> Vec<(GridCoords, CellHighlight)> {
    annotate_failing_cells(area, validity, grids, ObstacleCell::is_placeable_ground)
}

/// On invalid placement, highlights cells outside the map or failing [`ObstacleCell::can_be_excavated`].
pub fn annotate_not_excavatable(
    _: MapObject,
    area: GridArea,
    validity: PlacementValidity,
    grids: &GridsCollectionParam,
) -> Vec<(GridCoords, CellHighlight)> {
    annotate_failing_cells(area, validity, grids, ObstacleCell::can_be_excavated)
}

fn validate_every_cell(area: GridArea, grids: &GridsCollectionParam, accepts: fn(&ObstacleCell) -> bool) -> PlacementValidity {
    if !area.fits_in(grids.obstacle_grid.bounds) {
        return PlacementValidity::Invalid;
    }
    if grids.reserved_coords.any_reserved(area) {
        return PlacementValidity::Invalid;
    }
    if !grids.obstacle_grid.cells_in(area).all(accepts) {
        return PlacementValidity::Invalid;
    }
    PlacementValidity::Valid
}

fn annotate_failing_cells(
    area: GridArea,
    validity: PlacementValidity,
    grids: &GridsCollectionParam,
    accepts: fn(&ObstacleCell) -> bool,
) -> Vec<(GridCoords, CellHighlight)> {
    if validity != PlacementValidity::Invalid {
        return vec![];
    }
    area.iter()
        .filter(|coords| !grids.obstacle_grid.get(*coords).is_some_and(accepts))
        .map(|coords| (coords, CellHighlight::Negative))
        .collect()
}
