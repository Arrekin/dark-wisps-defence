use bevy::prelude::*;

use game_core::prelude::{AutoGridSnap, EdgeSide, EdgeSides, GridArea, MapBound};
use grids::EmissionsGridSpreadAffector;

pub mod wall_style;

pub mod prelude {
    pub use super::{
        DarkOre, DarkOreAreaScanner, DarkOreInRange,
        ExcavationShape,
        ExpeditionZone,
        HasOreInScannerRange,
        QuantumField,
        QuantumFieldSolved,
        VolumeShape,
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
/// [`VolumeShape`] defines its slope and tunnel openings.
#[derive(Component)]
#[require(MapBound, EmissionsGridSpreadAffector)]
pub struct Wall;

/// Surface slope and tunnel openings shared by walls and underground excavations.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[component(immutable)]
pub enum VolumeShape {
    /// A flat top with tunnel openings on the specified sides. An empty set makes it solid.
    Flat { openings: EdgeSides },
    /// A solid ramp rising toward `high_side`, with no tunnel openings.
    Ramp { high_side: EdgeSide },
}
impl Default for VolumeShape {
    fn default() -> Self {
        Self::Flat { openings: EdgeSides::NONE }
    }
}
impl VolumeShape {
    /// The tunnel openings; none for a ramp.
    pub const fn openings(self) -> EdgeSides {
        match self {
            Self::Flat { openings } => openings,
            Self::Ramp { .. } => EdgeSides::NONE,
        }
    }

    /// Whether the shape has at least one tunnel opening.
    pub const fn is_hollow(self) -> bool {
        !self.openings().is_empty()
    }

    /// Encodes the shape for shaders in seven bits.
    ///
    /// Bits 0–2 hold the ramp code: 0 for flat, 1–4 for right, left, top, and bottom.
    /// Bits 3–6 hold the tunnel openings from [`EdgeSides::bits`], or zero for a ramp.
    /// This layout must match `assets/shaders/walls/styles.wesl` and the `ramp_up` mapping in
    /// `assets/shaders/walls/look.wesl`.
    pub const fn gpu_code(self) -> u32 {
        const OPENINGS_SHIFT: u32 = 3;
        match self {
            Self::Flat { openings } => (openings.bits() as u32) << OPENINGS_SHIFT,
            Self::Ramp { high_side: EdgeSide::Right } => 1,
            Self::Ramp { high_side: EdgeSide::Left } => 2,
            Self::Ramp { high_side: EdgeSide::Top } => 3,
            Self::Ramp { high_side: EdgeSide::Bottom } => 4,
        }
    }
}

/// Underground terrain created by excavation: an open pit, ramp or tunnel.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
#[component(immutable)]
pub enum ExcavationShape {
    #[default]
    Pit,
    Shaped(VolumeShape),
}
impl ExcavationShape {
    /// Returns whether this shape is a tunnel beneath an intact ground floor.
    pub const fn is_tunnel(self) -> bool {
        matches!(self, Self::Shaped(VolumeShape::Flat { .. }))
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
