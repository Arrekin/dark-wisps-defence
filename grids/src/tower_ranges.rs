use bevy::{platform::collections::HashSet, prelude::*};

use game_core::prelude::GridCoords;

use crate::{GridVersion, base::BaseGrid};

#[derive(Copy, Clone, Debug)]
pub enum FloodTowerRangeMode {
    Add,
    Remove,
}

pub type TowerRangesGrid = BaseGrid<HashSet<Entity>, GridVersion>;
impl TowerRangesGrid {
    pub fn add_tower(&mut self, coords: GridCoords, tower: Entity) {
        self[coords].insert(tower);
        self.version = self.version.wrapping_add(1);
    }
    pub fn remove_tower(&mut self, coords: GridCoords, tower: Entity) {
        self[coords].remove(&tower);
        self.version = self.version.wrapping_add(1);
    }
}
