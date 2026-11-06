use std::collections::VecDeque;

use bevy::prelude::*;

use game_core::prelude::GridVolumePosition;

pub mod base;
pub mod visited;
pub mod placement;
pub mod slot_index;

pub type GridVersion = u32;

#[derive(Component, Default)]
pub struct GridPath {
    pub grid_version: GridVersion,
    pub path: VecDeque<GridVolumePosition>,
}
impl GridPath {
    pub fn next_in_path(&self) -> Option<GridVolumePosition> {
        self.path.front().copied()
    }
    pub fn remove_first(&mut self) {
        self.path.pop_front();
    }
    pub fn is_empty(&self) -> bool {
        self.path.is_empty()
    }
    pub fn distance(&self) -> usize {
        self.path.len()
    }
    /// Returns the position `distance` steps ahead, or `None` beyond the path.
    /// Distance starts at 1 for the next step; callers must not pass 0.
    pub fn at_distance(&self, distance: usize) -> Option<GridVolumePosition> {
        self.path.get(distance - 1).copied()
    }
}

/// Adding or removing entities with this component causes full recalculation of the emission grid
#[derive(Component, Default)]
pub struct EmissionsGridSpreadAffector;

/// System set for energy supply grid computation (supplier changes → power flood).
/// Consumers that read `EnergySupplyGrid` should order after this set.
#[derive(SystemSet, Clone, Copy, Eq, PartialEq, Hash, Debug)]
pub struct EnergySupplySystems;

/// Systems whose commands add or remove map structures.
/// Schedule systems after this set when they need those changes applied.
#[derive(SystemSet, Clone, Copy, Eq, PartialEq, Hash, Debug)]
pub struct ObstacleChangeSystems;

pub mod prelude {
    pub use super::{
        EnergySupplySystems,
        GridPath,
        GridVersion,
    };
    pub use super::placement::ReservedCoords;
}
