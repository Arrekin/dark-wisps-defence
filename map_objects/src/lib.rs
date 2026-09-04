use bevy::prelude::*;

use game_core::prelude::{GridCoords, GridImprint, MapBound, ZDepth};
use grids::{prelude::ObstacleGridObject, EmissionsGridSpreadAffector};

pub mod wall_style;

/// Drawn by the dark ore canvas from `GridCoords`.
#[derive(Component)]
#[require(MapBound, ObstacleGridObject = ObstacleGridObject::DarkOre)]
pub struct DarkOre {
    pub amount: i32,
}

/// Requests a dark-ore tooltip anchored to the contained tile entity.
#[derive(Component, Clone, Copy, Debug)]
pub struct BuilderDarkOreSideMenuTooltip(pub Entity);

/// Drawn by the wall canvas from `GridCoords`, so the entity itself carries no transform or render layer.
#[derive(Component)]
#[require(MapBound, ObstacleGridObject = ObstacleGridObject::Wall, EmissionsGridSpreadAffector)]
pub struct Wall;

/// Requests the wall UI material on an already-sized node.
#[derive(Component, Clone, Copy, Debug, Default)]
pub struct BuilderWallFace;

/// Requests a wall tooltip anchored to the contained tile entity.
#[derive(Component, Clone, Copy, Debug)]
pub struct BuilderWallSideMenuTooltip(pub Entity);

/// Requests the quantum-field UI material on an already-sized node.
#[derive(Component, Clone, Copy, Debug, Default)]
pub struct BuilderQuantumFieldFace;

/// Requests a quantum-field tooltip anchored to the contained tile entity.
#[derive(Component, Clone, Copy, Debug)]
pub struct BuilderQuantumFieldSideMenuTooltip(pub Entity);

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
pub struct QuantumFieldSolved;

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
        BuilderDarkOreSideMenuTooltip,
        BuilderQuantumFieldFace,
        BuilderQuantumFieldSideMenuTooltip,
        BuilderWallFace,
        BuilderWallSideMenuTooltip,
        DarkOre, DarkOreAreaScanner, DarkOreInRange,
        ExpeditionZone,
        HasOreInScannerRange,
        NoOreInScannerRange,
        QuantumField,
        QuantumFieldSolved,
        Wall,
    };
}
