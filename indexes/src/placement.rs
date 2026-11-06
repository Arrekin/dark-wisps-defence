//! Placement state and callbacks that read spatial indexes.
//! Shared placement events, modes, and styles live in `grids::placement`.

use bevy::{ecs::system::SystemParam, prelude::*};

use game_core::prelude::{AutoGridSnap, DrawLevel, GridArea, GridCoords, GridImprint, MapObject, ZDepth};
use grids::placement::{CellHighlight, PlacementDirection, PlacementModes, PlacementStyle, PlacementValidity, ReservedCoords};

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
#[require(GridArea, PlacementStyle, PlacementDirection, ZDepth::GridPlacer, DrawLevel::Airborne, AutoGridSnap)]
pub struct GridObjectPlacer {
    pub active_placement: Option<ActivePlacement>,
}

impl GridObjectPlacer {
    pub fn map_object(&self) -> Option<MapObject> {
        self.active_placement.as_ref().map(|active_placement| active_placement.map_object)
    }
}

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

/// Accepts an area only if every cell is in bounds, unreserved, and empty in the obstacle grid.
pub fn validator_all_empty(
    _: MapObject,
    area: GridArea,
    grids: &GridsCollectionParam,
) -> PlacementValidity {
    if !area.fits_in(grids.obstacle_grid.bounds) {
        return PlacementValidity::Invalid;
    }
    if grids.reserved_coords.any_reserved(area) {
        return PlacementValidity::Invalid;
    }
    if !grids.obstacle_grid.cells_in(area).all(ObstacleCell::is_empty) {
        return PlacementValidity::Invalid;
    }
    PlacementValidity::Valid
}

/// Highlights occupied cells and cells outside the map when placement is invalid.
pub fn annotate_non_empty(
    _: MapObject,
    area: GridArea,
    validity: PlacementValidity,
    grids: &GridsCollectionParam,
) -> Vec<(GridCoords, CellHighlight)> {
    if validity != PlacementValidity::Invalid {
        return vec![];
    }
    area.iter()
        .filter(|coords| !grids.obstacle_grid.get(*coords).is_some_and(ObstacleCell::is_empty))
        .map(|coords| (coords, CellHighlight::Negative))
        .collect()
}
