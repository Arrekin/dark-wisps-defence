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
        StructureShape,
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
/// [`StructureShape`] defines its slope and hollowed sides.
#[derive(Component)]
#[require(MapBound, EmissionsGridSpreadAffector)]
pub struct Wall;

/// Geometry of a wall or shaped underground excavation.
///
/// Both variants support a floor at the level above them, but differ in how they open at their own
/// level:
/// - **Hollowed sides** ([`Self::hollowed_sides`]) are passages carved underneath a flat top.
/// - **Open sides** ([`Self::open_sides`]) are cell edges open at the structure's own level: every
///   hollowed side of a [`Self::Flat`] shape, or the low edge (foot) of a [`Self::Ramp`] where the
///   slope meets the floor below.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[component(immutable)]
pub enum StructureShape {
    /// Full-height block with a flat top and optional passages carved through `hollowed_sides`.
    /// Empty `hollowed_sides` represents a solid wall or a refilled excavation.
    Flat { hollowed_sides: EdgeSides },
    /// Solid slope rising toward `high_side` from the floor of its own level to the floor above.
    Ramp { high_side: EdgeSide },
}
impl Default for StructureShape {
    fn default() -> Self {
        Self::Flat { hollowed_sides: EdgeSides::NONE }
    }
}
impl StructureShape {
    /// Sides with a passage carved underneath the top (`EdgeSides::NONE` for ramps).
    pub const fn hollowed_sides(self) -> EdgeSides {
        match self {
            Self::Flat { hollowed_sides } => hollowed_sides,
            Self::Ramp { .. } => EdgeSides::NONE,
        }
    }

    /// Sides open at the structure's own level: `hollowed_sides` for [`Self::Flat`], or the foot
    /// (`high_side.opposite()`) for [`Self::Ramp`].
    pub const fn open_sides(self) -> EdgeSides {
        match self {
            Self::Flat { hollowed_sides } => hollowed_sides,
            Self::Ramp { high_side } => EdgeSides::NONE.with(high_side.opposite()),
        }
    }

    /// True if at least one side has a carved passage.
    pub const fn is_hollow(self) -> bool {
        !self.hollowed_sides().is_empty()
    }

    /// Packs the shape into 7 bits for GPU buffers (`assets/shaders/grids/structure_shape.wesl`):
    /// bits 0..=2 store the ramp code (`0` = flat, `1..=4` = right, left, top, bottom), and
    /// bits 3..=6 store [`EdgeSides::bits`] for flat shapes (`0` for ramps).
    pub const fn gpu_code(self) -> u32 {
        const HOLLOWED_SIDES_SHIFT: u32 = 3;
        match self {
            Self::Flat { hollowed_sides } => (hollowed_sides.bits() as u32) << HOLLOWED_SIDES_SHIFT,
            Self::Ramp { high_side: EdgeSide::Right } => 1,
            Self::Ramp { high_side: EdgeSide::Left } => 2,
            Self::Ramp { high_side: EdgeSide::Top } => 3,
            Self::Ramp { high_side: EdgeSide::Bottom } => 4,
        }
    }
}

/// Underground excavation state: an open-air [`Self::Pit`] or a [`Self::Shaped`] ramp or flat
/// excavation.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
#[component(immutable)]
pub enum ExcavationShape {
    #[default]
    Pit,
    Shaped(StructureShape),
}
impl ExcavationShape {
    /// True if the ground floor above the excavation remains intact ([`StructureShape::Flat`]).
    pub const fn keeps_ground_floor(self) -> bool {
        matches!(self, Self::Shaped(StructureShape::Flat { .. }))
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
