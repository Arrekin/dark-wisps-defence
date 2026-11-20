//! # Altitude levels
//!
//! Each map cell has three levels: underground, on the ground, and on wall tops. A level's floor
//! exists when the level below provides one. Floors hold effects such as napalm; the space above
//! holds structures and units. The terrain model limits terrain to these three levels.
//!
//! [`AltitudeLevel`] also names airborne objects, effects spanning all levels, and areas that reach
//! the top of each cell. Each index and consumer decides what each level means for it. These
//! levels describe map occupancy; rendering uses [`DrawLevel`](crate::z_depth::DrawLevel) for draw
//! order.

use strum::{AsRefStr, EnumCount, EnumIter, EnumString};

/// The altitude occupied by an object or effect.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, EnumIter, EnumCount, EnumString, AsRefStr)]
pub enum AltitudeLevel {
    /// Underground terrain, passages, units, and floor covers.
    Underground,
    /// Structures, units, and floor covers at ground level, including walls, buildings, and napalm.
    #[default]
    OnGround,
    /// Objects, units, and floor covers on wall tops.
    OnElevation,
    /// Objects above the terrain, such as drones, projectiles, and domes.
    Airborne,
    /// Effects spanning all levels, such as quantum fields, energy supply, and dome coverage.
    AllEncompassing,
    /// Areas that reach the top of each cell, such as flame streams. Each cell resolves its own top when read.
    TopLevel,
}
