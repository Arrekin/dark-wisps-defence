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
    /// Converts this altitude to a volume level.
    ///
    /// # Panics
    ///
    /// Panics for any level that is not a volume.
    pub fn volume(self) -> VolumeLevel {
        match self {
            Self::Underground => VolumeLevel::Underground,
            Self::Elevation => VolumeLevel::Elevation,
            Self::OnElevation => VolumeLevel::OnElevation,
            other => panic!("{other:?} is not a volume"),
        }
    }

    /// Converts this altitude to a floor level.
    ///
    /// # Panics
    ///
    /// Panics for any level that is not a floor.
    pub fn floor(self) -> FloorLevel {
        match self {
            Self::UndergroundFloor => FloorLevel::UndergroundFloor,
            Self::GroundFloor => FloorLevel::GroundFloor,
            Self::ElevationFloor => FloorLevel::ElevationFloor,
            other => panic!("{other:?} is not a floor"),
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

    /// The floor directly below this volume.
    pub const fn floor_below(self) -> FloorLevel {
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
    pub const fn volume_above(self) -> VolumeLevel {
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
