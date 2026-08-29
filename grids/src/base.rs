use std::ops::{Index, IndexMut};

use bevy::prelude::*;

use game_core::prelude::{Bounds, GridCoords};

use crate::{FieldTrait, GridVersionTrait};

#[derive(Resource)]
pub struct BaseGrid<FieldType, GridVersionType> where FieldType: FieldTrait, GridVersionType: GridVersionTrait {
    pub bounds: Bounds,
    pub grid: Vec<FieldType>,
    pub version: GridVersionType, // Used to determine whether the grid has changed
}

impl<FieldType, GridVersionType> BaseGrid<FieldType, GridVersionType> where FieldType: FieldTrait, GridVersionType: GridVersionTrait {
    pub fn new_empty() -> Self {
        Self {
            bounds: Bounds::default(),
            grid: vec![],
            version: GridVersionType::default(),
        }
    }
    pub fn new_with_size(bounds: impl Into<Bounds>) -> Self {
        let bounds = bounds.into();
        Self {
            bounds,
            grid: vec![Default::default(); bounds.area()],
            version: GridVersionType::default(),
        }
    }
    pub fn resize_and_reset(&mut self, bounds: impl Into<Bounds>) {
        let bounds = bounds.into();
        if self.bounds != bounds {
            self.bounds = bounds;
            self.grid.resize(bounds.area(), Default::default());
        }
        self.reset();
    }
    pub fn reset(&mut self) {
        self.grid.fill(Default::default());
    }
    pub fn index(&self, coords: GridCoords) -> usize {
        self.bounds.index(coords)
    }
}


impl<FieldType, GridVersionType> Index<GridCoords> for BaseGrid<FieldType, GridVersionType> where FieldType: FieldTrait, GridVersionType: GridVersionTrait {
    type Output = FieldType;

    fn index(&self, coords: GridCoords) -> &Self::Output {
        let index = self.index(coords);
        &self.grid[index]
    }
}
impl<FieldType, GridVersionType>  IndexMut<GridCoords> for BaseGrid<FieldType, GridVersionType> where FieldType: FieldTrait, GridVersionType: GridVersionTrait {
    fn index_mut(&mut self, coords: GridCoords) -> &mut Self::Output {
        let index = self.index(coords);
        &mut self.grid[index]
    }
}
