use bevy::prelude::*;

use game_core::prelude::{AutoGridSnap, EdgeSide, GridArea, MapBound};
use grids::EmissionsGridSpreadAffector;

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
#[require(MapBound)]
pub struct DarkOre {
    pub amount: i32,
}

/// Drawn by the wall canvas from `GridArea`, so the entity itself carries no transform or render layer.
#[derive(Component)]
#[require(MapBound, EmissionsGridSpreadAffector)]
pub struct Wall {
    /// The ramp's high side, or `None` for a flat wall.
    pub high_side: Option<EdgeSide>,
}
impl Wall {
    /// Encodes the ramp direction for shaders; 0 represents a flat wall.
    /// Mirrors `ramp_up` in `assets/shaders/walls/look.wesl`.
    pub const fn ramp_gpu_code(high_side: Option<EdgeSide>) -> u32 {
        match high_side {
            None => 0,
            Some(EdgeSide::Right) => 1,
            Some(EdgeSide::Left) => 2,
            Some(EdgeSide::Top) => 3,
            Some(EdgeSide::Bottom) => 4,
        }
    }
}

/// Progressive obstacle requiring drone scanning to solve.
#[derive(Component, Default)]
#[require(AutoGridSnap, MapBound)]
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
