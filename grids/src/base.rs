use std::ops::{Index, IndexMut};

use bevy::prelude::*;

use game_core::prelude::GridCoords;

use crate::{FieldTrait, GridVersionTrait};

#[derive(Resource)]
pub struct BaseGrid<FieldType, GridVersionType> where FieldType: FieldTrait, GridVersionType: GridVersionTrait {
    pub width: i32,
    pub height: i32,
    pub grid: Vec<FieldType>,
    pub version: GridVersionType, // Used to determine whether the grid has changed
}

impl<FieldType, GridVersionType> BaseGrid<FieldType, GridVersionType> where FieldType: FieldTrait, GridVersionType: GridVersionTrait {
    pub fn new_empty() -> Self {
        Self {
            width: 0,
            height: 0,
            grid: vec![],
            version: GridVersionType::default(),
        }
    }
    pub fn new_with_size(width: i32, height: i32) -> Self {
        Self {
            width,
            height,
            grid: vec![Default::default(); (width * height) as usize],
            version: GridVersionType::default(),
        }
    }
    pub fn resize_and_reset(&mut self, bounds: (i32, i32)) {
        if self.bounds() != bounds {
            self.width = bounds.0;
            self.height = bounds.1;
            self.grid.resize((bounds.0 * bounds.1) as usize, Default::default());
        }
        self.reset();
    }
    pub fn reset(&mut self) {
        self.grid.fill(Default::default());
    }
    pub fn index(&self, coords: GridCoords) -> usize {
        (coords.y * self.width + coords.x) as usize
    }
    pub fn bounds(&self) -> (i32, i32) {
        (self.width, self.height)
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
