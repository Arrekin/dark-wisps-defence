use bevy::{platform::collections::HashSet, prelude::*};

use game_core::prelude::{BuildingType, GridArea, GridCoords};

use crate::{GridVersion, base::BaseGrid};

#[derive(Clone, Debug, PartialEq, Default)]
pub struct ObstacleCell {
    pub dark_ore: Option<Entity>,
    pub quantum_field: Option<Entity>,
    pub structure: GridStructureType,
}
impl ObstacleCell {
    pub fn has_dark_ore(&self) -> bool {
        self.dark_ore.is_some()
    }
    pub fn is_within_quantum_field(&self) -> bool {
        self.quantum_field.is_some()
    }
    pub fn is_empty(&self) -> bool {
        matches!(self.structure, GridStructureType::Empty) && !self.is_within_quantum_field() && !self.has_dark_ore()
    }
    pub fn has_building(&self) -> bool {
        matches!(self.structure, GridStructureType::Building(..))
    }
    pub fn has_wall(&self) -> bool {
        matches!(self.structure, GridStructureType::Wall(_))
    }
    pub fn has_structure(&self) -> bool {
        matches!(self.structure, GridStructureType::Wall(_) | GridStructureType::Building(..))
    }
}

#[derive(Clone, Debug, PartialEq, Default)]
pub enum GridStructureType {
    #[default]
    Empty,
    Building(Entity, BuildingType),
    Wall(Entity),
}

pub type ObstacleGrid = BaseGrid<ObstacleCell, GridVersion>;

impl ObstacleGrid {
    pub fn imprint_structure(&mut self, area: GridArea, structure: GridStructureType) {
        area.iter().for_each(|cell| self[cell].structure = structure.clone());
        self.version = self.version.wrapping_add(1);
    }
    pub fn deprint_structure(&mut self, area: GridArea) {
        area.iter().for_each(|cell| self[cell].structure = GridStructureType::Empty);
        self.version = self.version.wrapping_add(1);
    }
    /// True if `query` returns true for every in-bounds cell of the area.
    pub fn query_area_all(&self, area: GridArea, query: fn(&ObstacleCell) -> bool) -> bool {
        area.iter_in_bounds(self.bounds).all(|cell| query(&self[cell]))
    }
    /// True if `query` returns true for at least one in-bounds cell of the area.
    pub fn query_area_any(&self, area: GridArea, query: fn(&ObstacleCell) -> bool) -> bool {
        area.iter_in_bounds(self.bounds).any(|cell| query(&self[cell]))
    }
    /// Number of in-bounds cells for which `query` returns true.
    pub fn query_area_count(&self, area: GridArea, query: fn(&ObstacleCell) -> bool) -> usize {
        area.iter_in_bounds(self.bounds).filter(|cell| query(&self[*cell])).count()
    }
    /// Collects the non-None results of `query` over every in-bounds cell.
    pub fn query_area_element<T>(&self, area: GridArea, query: fn(&ObstacleCell) -> Option<T>) -> Vec<T> {
        area.iter_in_bounds(self.bounds).filter_map(|cell| query(&self[cell])).collect()
    }

    pub fn imprint_custom(&mut self, area: GridArea, imprint_fn: impl Fn(&mut ObstacleCell)) {
        area.iter().for_each(|cell| imprint_fn(&mut self[cell]));
        self.version = self.version.wrapping_add(1);
    }
}

/// When placing objects on map sometimes we want to reserve space so the state can be changed async later while ensuring that no other object can be placed there in parallel systems.
/// These reservations are cleared in the First schedule of the following frame.
#[derive(Resource, Default)]
pub struct ReservedCoords {
    pub for_structures: HashSet<GridCoords>,
}
impl ReservedCoords {
    pub fn reserve(&mut self, area: GridArea) {
        self.for_structures.extend(area);
    }
    pub fn any_reserved(&self, area: GridArea) -> bool {
        area.iter().any(|cell| self.for_structures.contains(&cell))
    }
}
