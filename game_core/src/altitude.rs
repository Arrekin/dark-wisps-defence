//! # Altitude levels
//!
//! Each map cell has three floor/volume pairs: tunnel floor and underground space, ground floor
//! and ground-level space, and wall top and the space above it. Floors hold effects such as
//! napalm; volumes hold structures and units. The terrain model treats a floor as supported by
//! the volume below it and limits terrain to these three pairs.
//!
//! [`AltitudeLevel`] includes these six levels, airborne objects, effects spanning all levels, and
//! areas that reach the top of each cell.
//! [`VolumeLevel`] and [`FloorLevel`] restrict APIs to volumes or floors. These types describe
//! map occupancy; rendering uses [`DrawLevel`](crate::z_depth::DrawLevel) for draw order.

use strum::{AsRefStr, EnumCount, EnumIter, EnumString};

/// The altitude occupied by an object or effect.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, EnumIter, EnumCount, EnumString, AsRefStr)]
pub enum AltitudeLevel {
    /// Floor covers on the tunnel floor.
    UndergroundFloor,
    /// Underground terrain, passages, and units.
    Underground,
    /// Floor covers on the ground, such as napalm and char.
    GroundFloor,
    /// Structures and units at ground level, including walls and buildings.
    #[default]
    Elevation,
    /// Floor covers on wall tops.
    ElevationFloor,
    /// Objects and units on wall tops.
    OnElevation,
    /// Objects above the terrain, such as drones, projectiles, and domes.
    Airborne,
    /// Effects spanning all levels, such as quantum fields, energy supply, and dome coverage.
    AllEncompassing,
    /// Areas that reach the top of each cell, such as blasts. Each cell resolves its own top when read.
    TopLevel,
}

impl AltitudeLevel {
    /// Converts this altitude to the volume that holds it. A floor belongs to the volume above it;
    /// `Airborne`, `AllEncompassing` and `TopLevel` belong to the highest volume.
    pub const fn volume(self) -> VolumeLevel {
        match self {
            Self::UndergroundFloor => VolumeLevel::Underground,
            Self::Underground => VolumeLevel::Underground,
            Self::GroundFloor => VolumeLevel::Elevation,
            Self::Elevation => VolumeLevel::Elevation,
            Self::ElevationFloor => VolumeLevel::OnElevation,
            Self::OnElevation => VolumeLevel::OnElevation,
            Self::Airborne => VolumeLevel::OnElevation,
            Self::AllEncompassing => VolumeLevel::OnElevation,
            Self::TopLevel => VolumeLevel::OnElevation,
        }
    }

    /// Converts this altitude to the floor that holds it. A volume stands on its own floor;
    /// `Airborne`, `AllEncompassing` and `TopLevel` belong to the highest floor.
    pub const fn floor(self) -> FloorLevel {
        match self {
            Self::UndergroundFloor => FloorLevel::UndergroundFloor,
            Self::Underground => FloorLevel::UndergroundFloor,
            Self::GroundFloor => FloorLevel::GroundFloor,
            Self::Elevation => FloorLevel::GroundFloor,
            Self::ElevationFloor => FloorLevel::ElevationFloor,
            Self::OnElevation => FloorLevel::ElevationFloor,
            Self::Airborne => FloorLevel::ElevationFloor,
            Self::AllEncompassing => FloorLevel::ElevationFloor,
            Self::TopLevel => FloorLevel::ElevationFloor,
        }
    }
}

/// Space above a floor, with room for one structure and multiple units per cell.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, EnumIter, EnumCount)]
pub enum VolumeLevel {
    Underground,
    #[default]
    Elevation,
    OnElevation,
}
impl VolumeLevel {
    /// Returns the volume directly above, or `None` at `OnElevation`.
    pub const fn above(self) -> Option<Self> {
        match self {
            Self::Underground => Some(Self::Elevation),
            Self::Elevation => Some(Self::OnElevation),
            Self::OnElevation => None,
        }
    }

    /// Returns the volume directly below, or `None` at `Underground`.
    pub const fn below(self) -> Option<Self> {
        match self {
            Self::Underground => None,
            Self::Elevation => Some(Self::Underground),
            Self::OnElevation => Some(Self::Elevation),
        }
    }

    /// The floor at the bottom of this volume.
    pub const fn floor(self) -> FloorLevel {
        match self {
            Self::Underground => FloorLevel::UndergroundFloor,
            Self::Elevation => FloorLevel::GroundFloor,
            Self::OnElevation => FloorLevel::ElevationFloor,
        }
    }
}

/// A surface that can hold multiple floor cover effects per cell.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, EnumIter, EnumCount)]
pub enum FloorLevel {
    UndergroundFloor,
    #[default]
    GroundFloor,
    ElevationFloor,
}
impl FloorLevel {
    /// GPU code matching `FLOOR_*` in `assets/shaders/grids/floor_cover.wesl`.
    pub const fn gpu_index(self) -> u32 {
        match self {
            Self::UndergroundFloor => 0,
            Self::GroundFloor => 1,
            Self::ElevationFloor => 2,
        }
    }

    /// The volume directly above this floor.
    pub const fn volume(self) -> VolumeLevel {
        match self {
            Self::UndergroundFloor => VolumeLevel::Underground,
            Self::GroundFloor => VolumeLevel::Elevation,
            Self::ElevationFloor => VolumeLevel::OnElevation,
        }
    }
}

impl From<VolumeLevel> for AltitudeLevel {
    fn from(level: VolumeLevel) -> Self {
        match level {
            VolumeLevel::Underground => Self::Underground,
            VolumeLevel::Elevation => Self::Elevation,
            VolumeLevel::OnElevation => Self::OnElevation,
        }
    }
}
impl From<FloorLevel> for AltitudeLevel {
    fn from(level: FloorLevel) -> Self {
        match level {
            FloorLevel::UndergroundFloor => Self::UndergroundFloor,
            FloorLevel::GroundFloor => Self::GroundFloor,
            FloorLevel::ElevationFloor => Self::ElevationFloor,
        }
    }
}
