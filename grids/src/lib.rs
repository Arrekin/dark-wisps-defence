use std::{collections::VecDeque, fmt::Debug};

use bevy::prelude::*;

use game_core::prelude::GridCoords;

pub mod base;
pub mod visited;
pub mod placement;
pub mod search;
pub mod slot_index;

pub type GridVersion = u32;

pub trait CellTrait: Default + Clone + Debug {}
impl<T: Default + Clone + Debug> CellTrait for T {}

pub trait GridVersionTrait: Default {}
impl<T: Default> GridVersionTrait for T {}

#[derive(Component, Default)]
pub struct GridPath {
    pub grid_version: GridVersion,
    pub path: VecDeque<GridCoords>,
}
impl GridPath {
    pub fn next_in_path(&self) -> Option<GridCoords> {
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
    pub fn at_distance(&self, index: usize) -> Option<GridCoords> {
        self.path.get(index - 1).copied()
    }
}

/// Adding or removing entities with this component causes full recalculation of the emission grid
#[derive(Component, Default)]
pub struct EmissionsGridSpreadAffector;

/// System set for energy supply grid computation (supplier changes → power flood).
/// Consumers that read `EnergySupplyGrid` should order after this set.
#[derive(SystemSet, Clone, Copy, Eq, PartialEq, Hash, Debug)]
pub struct EnergySupplySystems;

pub mod prelude {
    pub use super::{
        EnergySupplySystems,
        GridPath,
        GridVersion,
    };
    pub use super::placement::ReservedCoords;
}
