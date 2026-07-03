use std::{collections::VecDeque, fmt::Debug};

use bevy::prelude::*;

use game_core::prelude::GridCoords;

pub mod base;
pub mod obstacles;
pub mod tower_ranges;
pub mod energy_supply;
pub mod wisps;
pub mod force_fields;
pub mod visited;
pub mod emissions;
pub mod map_info;
pub mod placement;
pub mod search;

pub type GridVersion = u32;

pub trait FieldTrait: Default + Clone + Debug {}
impl<T: Default + Clone + Debug> FieldTrait for T {}

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

/// Component auto-managing given entity presence in ObstacleGrid
#[derive(Component)]
pub enum ObstacleGridObject {
    Building,
    Wall,
    QuantumField,
    DarkOre,
}

/// Adding or removing entities with this component causes full recalculation of the emission grid
#[derive(Component, Default)]
pub struct EmissionsGridSpreadAffector;

// Marker: the entity's `Transform` is auto-synced to its `GridCoords` + `GridImprint` world position.
#[derive(Component, Default)]
pub struct AutoGridTransformSync;

pub mod prelude {
    pub use super::{
        EmissionsGridSpreadAffector,
        GridPath,
        GridVersion,
        ObstacleGridObject,
    };
    pub use super::emissions::{
        EmissionsEnergyRecalculateAll,
        EmissionsGrid,
        EmissionsType,
        EmitterChangedEvent,
        EmitterEnergy,
        EmitterEnergyEnabled,
        FloodEmissionsDetails,
        FloodEmissionsEvaluator,
        FloodEmissionsMode,
        FloodTowerRangeMode,
    };
    pub use super::energy_supply::{
        FloodEnergySupplyMode,
        GeneratorEnergy,
        HasPower,
        NeedsPower,
        NoPower,
        SupplierChangedEvent,
    };
    pub use super::map_info::MapInfo;
    pub use super::obstacles::{Field, ObstacleGrid, ReservedCoords};
    pub use super::placement::{
        CellHighlight,
        GridObjectPlacer,
        GridObjectPlacerRequest,
        GridPlacerChanged,
        GridsCollectionParam,
        ObjectPlacementInfo,
        PlaceRequest,
        PlacementEmitter,
        PlacementMode,
        PlacementValidity,
        RemoveRequest,
    };
    pub use super::wisps::WispsGrid;
}
