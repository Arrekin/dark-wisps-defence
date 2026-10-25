use bevy::prelude::*;

use game_core::prelude::{AutoGridSnap, GridArea, MapBound};
use grids::{prelude::ObstacleGridObject, EmissionsGridSpreadAffector};

pub mod wall_style;

pub mod prelude {
    pub use super::{
        DarkOre, DarkOreAreaScanner, DarkOreInRange,
        ExpeditionZone,
        HasOreInScannerRange,
        QuantumField,
        QuantumFieldSolved,
        Wall,
    };
}

/// Drawn by the dark ore canvas from `GridArea`.
#[derive(Component)]
#[require(MapBound, ObstacleGridObject = ObstacleGridObject::DarkOre)]
pub struct DarkOre {
    pub amount: i32,
}

/// Drawn by the wall canvas from `GridArea`, so the entity itself carries no transform or render layer.
#[derive(Component)]
#[require(MapBound, ObstacleGridObject = ObstacleGridObject::Wall, EmissionsGridSpreadAffector)]
pub struct Wall;

/// Progressive obstacle requiring drone scanning to solve.
#[derive(Component, Default)]
#[require(AutoGridSnap, MapBound, ObstacleGridObject = ObstacleGridObject::QuantumField)]
pub struct QuantumField;

/// Accumulates expedition-drone scan progress while a beam is active over the entity.
/// The target's domain consumes progress through `take_accumulated_scan_progress`.
#[derive(Component, Default)]
pub struct ExpeditionZone {
    pub accumulated_scan_progress: f32,
}
impl ExpeditionZone {
    /// Consumes and returns accumulated progress, resetting to zero.
    pub fn take_accumulated_scan_progress(&mut self) -> f32 {
        let progress = self.accumulated_scan_progress;
        self.accumulated_scan_progress = 0.0;
        progress
    }
}

/// Marks a quantum field whose layers are all solved.
#[derive(Component)]
pub struct QuantumFieldSolved;

/// Present while the scanner's `DarkOreInRange` is not empty.
#[derive(Component, Default)]
pub struct HasOreInScannerRange;

#[derive(Component, Default)]
pub struct DarkOreInRange(pub Vec<Entity>);

#[derive(Component, Clone)]
#[component(immutable)]
#[require(DarkOreInRange)]
pub struct DarkOreAreaScanner {
    /// Cells scanned for dark ore, on the map.
    pub range: GridArea,
}
