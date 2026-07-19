use bevy::prelude::*;

use game_core::prelude::{GridCoords, GridImprint, MapBound, ZDepth};
use grids::{prelude::ObstacleGridObject, EmissionsGridSpreadAffector};

#[derive(Component)]
#[require(MapBound, ObstacleGridObject = ObstacleGridObject::DarkOre, ZDepth::OBSTACLE)]
pub struct DarkOre {
    pub amount: i32,
}


#[derive(Component)]
#[require(MapBound, ObstacleGridObject = ObstacleGridObject::Wall, EmissionsGridSpreadAffector, ZDepth::OBSTACLE)]
pub struct Wall;

/// Progressive obstacle requiring drone scanning to solve.
/// Layers are defined at spawn time; current_layer indexes into the layers vec.
#[derive(Component, Default)]
#[require(MapBound, ObstacleGridObject = ObstacleGridObject::QuantumField, ZDepth::OBSTACLE)]
pub struct QuantumField;

/// Marks an entity as a valid target for expedition drones.
///
/// Drones accumulate scan progress here while their beam is active over this entity.
/// The owning system (e.g., QuantumField) should poll and consume progress via
/// `take_accumulated_scan_progress()` to drive its own mechanics.
///
/// This decoupling allows different entity types to use drone scanning without
/// the drone system needing to know about their specific progression logic.
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

/// Marker for fully-solved QuantumFields. Removes ExpeditionZone to prevent further scanning.
#[derive(Component)]
pub struct Solved;

#[derive(Component, Default)]
pub struct HasOreInScannerRange;
#[derive(Component, Default)]
pub struct NoOreInScannerRange;

#[derive(Component, Default)]
pub struct DarkOreInRange(pub Vec<Entity>);

#[derive(Component, Clone)]
#[component(immutable)]
#[require(GridCoords, DarkOreInRange, NoOreInScannerRange)]
pub struct DarkOreAreaScanner {
    pub range_imprint: GridImprint,
}

pub mod prelude {
    pub use super::{
        DarkOre, DarkOreAreaScanner, DarkOreInRange,
        ExpeditionZone,
        HasOreInScannerRange,
        NoOreInScannerRange,
        QuantumField,
        Solved,
        Wall,
    };
}
