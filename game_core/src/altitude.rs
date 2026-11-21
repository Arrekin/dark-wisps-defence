//! # Altitude levels
//!
//! Each map cell has three levels: underground, on the ground, and on wall tops. A level's floor
//! exists when the level below provides one. Floors hold effects such as napalm; the space above
//! holds structures and units. The terrain model limits terrain to these three levels.
//!
//! [`AltitudeLevel`] also names airborne objects, effects spanning all levels, and areas that reach
//! the top of each cell. [`AltitudeLevel::three_layer_index`] maps every variant onto the three
//! terrain layers (`Underground`, `OnGround`, `OnElevation`); indexes and consumers may use this
//! default mapping or define their own. These levels describe map occupancy; rendering uses
//! [`DrawLevel`](crate::z_depth::DrawLevel) for draw order.

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
impl AltitudeLevel {
    /// The three terrain layers ordered from bottom to top, indexed by [`Self::three_layer_index`].
    pub const THREE_LAYERS: [Self; 3] = [Self::Underground, Self::OnGround, Self::OnElevation];

    /// Maps this level to its index in [`Self::THREE_LAYERS`]: `0` for `Underground`, `1` for
    /// `OnGround`, and `2` for `OnElevation` and all higher or spanning variants.
    pub const fn three_layer_index(self) -> usize {
        match self {
            Self::Underground => 0,
            Self::OnGround => 1,
            Self::OnElevation | Self::Airborne | Self::AllEncompassing | Self::TopLevel => 2,
        }
    }

    /// Returns [`Self::three_layer_index`] as a `u32` for GPU buffers.
    pub const fn three_layer_index_gpu(self) -> u32 {
        self.three_layer_index() as u32
    }

    /// Returns the terrain layer directly above this one, or `None` at `OnElevation` and above.
    pub const fn three_layer_above(self) -> Option<Self> {
        let index = self.three_layer_index();
        if index + 1 < Self::THREE_LAYERS.len() { Some(Self::THREE_LAYERS[index + 1]) } else { None }
    }

    /// Returns the terrain layer directly below this one, or `None` at `Underground`.
    pub const fn three_layer_below(self) -> Option<Self> {
        let index = self.three_layer_index();
        if index > 0 { Some(Self::THREE_LAYERS[index - 1]) } else { None }
    }
}
