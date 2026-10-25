use std::ops::{Index, IndexMut};

use bevy::prelude::*;

use game_core::prelude::{Bounds, GridCoords};

use crate::{CellTrait, GridVersionTrait};

#[derive(Resource, Default)]
pub struct BaseGrid<CellType, GridVersionType> where CellType: CellTrait, GridVersionType: GridVersionTrait {
    pub bounds: Bounds,
    pub grid: Vec<CellType>,
    pub version: GridVersionType, // Used to determine whether the grid has changed
}

impl<CellType, GridVersionType> BaseGrid<CellType, GridVersionType> where CellType: CellTrait, GridVersionType: GridVersionTrait {
    pub fn new_with_size(bounds: impl Into<Bounds>) -> Self {
        let bounds = bounds.into();
        Self {
            bounds,
            grid: vec![Default::default(); bounds.area()],
            version: GridVersionType::default(),
        }
    }
    /// Resizes only when the bounds change. Added cells start at their default.
    pub fn resize(&mut self, bounds: impl Into<Bounds>) {
        let bounds = bounds.into();
        if self.bounds != bounds {
            self.bounds = bounds;
            self.grid.resize(bounds.area(), Default::default());
        }
    }
    pub fn resize_and_reset(&mut self, bounds: impl Into<Bounds>) {
        self.resize(bounds);
        self.reset();
    }
    pub fn reset(&mut self) {
        self.grid.fill(Default::default());
    }
    pub fn index(&self, coords: GridCoords) -> usize {
        self.bounds.index(coords)
    }
    /// The cell at `coords`, or `None` outside the grid.
    pub fn get(&self, coords: GridCoords) -> Option<&CellType> {
        self.bounds.index_checked(coords).map(|index| &self.grid[index])
    }
}


impl<CellType, GridVersionType> Index<GridCoords> for BaseGrid<CellType, GridVersionType> where CellType: CellTrait, GridVersionType: GridVersionTrait {
    type Output = CellType;

    fn index(&self, coords: GridCoords) -> &Self::Output {
        let index = self.index(coords);
        &self.grid[index]
    }
}
impl<CellType, GridVersionType>  IndexMut<GridCoords> for BaseGrid<CellType, GridVersionType> where CellType: CellTrait, GridVersionType: GridVersionTrait {
    fn index_mut(&mut self, coords: GridCoords) -> &mut Self::Output {
        let index = self.index(coords);
        &mut self.grid[index]
    }
}
