use bevy::prelude::*;

use game_core::prelude::{AutoGridSnap, GridCoords, GridImprint, MapBound};
use grids::{prelude::ObstacleGridObject, EmissionsGridSpreadAffector};

pub mod wall_style;

pub mod prelude {
    pub use super::{
        BuilderDarkOreSideMenuTooltip,
        BuilderQuantumFieldSideMenuTooltip,
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

/// Drawn by the dark ore canvas from `GridCoords`.
#[derive(Component)]
#[require(MapBound, ObstacleGridObject = ObstacleGridObject::DarkOre)]
pub struct DarkOre {
    pub amount: i32,
}

/// Spawns a dark-ore tooltip anchored to the contained tile entity.
#[derive(Component, Clone, Copy, Debug)]
pub struct BuilderDarkOreSideMenuTooltip(pub Entity);

/// Drawn by the wall canvas from `GridCoords`, so the entity itself carries no transform or render layer.
#[derive(Component)]
#[require(MapBound, ObstacleGridObject = ObstacleGridObject::Wall, EmissionsGridSpreadAffector)]
pub struct Wall;

/// Spawns a wall tooltip anchored to the contained tile entity.
#[derive(Component, Clone, Copy, Debug)]
pub struct BuilderWallSideMenuTooltip(pub Entity);

/// Spawns a quantum-field tooltip anchored to the contained tile entity.
#[derive(Component, Clone, Copy, Debug)]
pub struct BuilderQuantumFieldSideMenuTooltip(pub Entity);

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
