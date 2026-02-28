use lib_core::placement::PlacementEmitter;
use lib_grid::grids::obstacles::{ObstacleGrid, ReservedCoords};
use lib_grid::grids::energy_supply::EnergySupplyGrid;

use crate::lib_prelude::*;


/// Result of placement validation. Placer uses this to set sprite color.
#[derive(Clone, Copy, Debug)]
pub struct PlacementValidationResult {
    pub can_place: bool,
    pub color: Color,
}
impl PlacementValidationResult {
    pub fn valid() -> Self {
        Self { can_place: true, color: Color::srgba(0.0, 1.0, 0.0, 0.2) }
    }
    pub fn valid_unpowered() -> Self {
        Self { can_place: true, color: Color::srgba(1.0, 1.0, 0.0, 0.2) }
    }
    pub fn invalid() -> Self {
        Self { can_place: false, color: Color::srgba(1.0, 0.0, 0.0, 0.2) }
    }
}

/// Static validator function that receives map information and placable object data to return decisions if placement is valid.
/// This is used for early placement feedback, in the end the domain handler makes final decision.
/// First argument is the MapObject being placed, followed by coords, imprint, and grid data.
pub type PlacementValidatorFn = fn(MapObject, GridCoords, GridImprint, &GridsCollection) -> PlacementValidationResult;

/// Grid data available to validators during placement validation.
pub struct GridsCollection<'a> {
    pub obstacle_grid: &'a ObstacleGrid,
    pub energy_supply_grid: &'a EnergySupplyGrid,
    pub reserved_coords: &'a ReservedCoords,
}

/// Whether placement triggers on press (burst mode) or release (single click).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PlacementMode {
    /// Emit on mouse button release (default, single placement per click)
    #[default]
    OnRelease,
    /// Emit while mouse button is held (burst mode for walls, etc.)
    OnPress,
}

/// Generic placement info extracted from Almanach. Placer stores this.
pub struct ObjectPlacementInfo {
    pub imprint: GridImprint,
    pub validate: PlacementValidatorFn,
    pub place_emitter: Box<dyn PlacementEmitter>,
    pub remove_emitter: Option<Box<dyn PlacementEmitter>>,
    pub place_mode: PlacementMode,
    pub remove_mode: PlacementMode,
}

