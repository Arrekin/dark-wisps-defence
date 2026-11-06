use std::ops::{Deref, DerefMut, Index, IndexMut};

use bevy::prelude::*;
use strum::EnumCount;

use game_core::prelude::{Bounds, GridArea, GridCoords, GridVolumePosition, VolumeLevel};

use crate::GridVersion;

/// Storage at one grid coordinate, with access by volume level.
pub trait GridCell: Default + Clone {
    /// The value exposed at a volume level.
    type Data;
    fn at(&self, level: VolumeLevel) -> &Self::Data;
    fn at_mut(&mut self, level: VolumeLevel) -> &mut Self::Data;
}

/// One value per [`VolumeLevel`].
pub type GridStack<T> = [T; VolumeLevel::COUNT];
impl<T: Default + Clone> GridCell for GridStack<T> {
    type Data = T;
    fn at(&self, level: VolumeLevel) -> &T { &self[level as usize] }
    fn at_mut(&mut self, level: VolumeLevel) -> &mut T { &mut self[level as usize] }
}

/// One value shared by all volume levels at a grid coordinate.
#[derive(Default, Clone, Debug)]
pub struct GridSingle<T>(pub T);
impl<T: Default + Clone> GridCell for GridSingle<T> {
    type Data = T;
    fn at(&self, _level: VolumeLevel) -> &T { &self.0 }
    fn at_mut(&mut self, _level: VolumeLevel) -> &mut T { &mut self.0 }
}
impl<T> Deref for GridSingle<T> {
    type Target = T;
    fn deref(&self) -> &T { &self.0 }
}
impl<T> DerefMut for GridSingle<T> {
    fn deref_mut(&mut self) -> &mut T { &mut self.0 }
}

/// A bounded grid with one [`GridCell`] per coordinate.
///
/// Indexing by [`GridVolumePosition`] selects a value within the cell. Use [`Self::cell`]
/// to access the entire cell. Grids of [`GridSingle`] also support indexing by [`GridCoords`].
#[derive(Resource, Default)]
pub struct BaseGrid<CellType, GridVersionType> {
    pub bounds: Bounds,
    pub grid: Vec<CellType>,
    /// Caller-managed counter: a change version for indexes or a generation stamp for searches.
    pub version: GridVersionType,
}

impl<CellType, GridVersionType> BaseGrid<CellType, GridVersionType> where CellType: GridCell {
    pub fn new_with_size(bounds: impl Into<Bounds>) -> Self where GridVersionType: Default {
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
    pub fn index_of(&self, coords: GridCoords) -> usize {
        self.bounds.index(coords)
    }
    /// Returns the entire cell at `coords`. Coordinates must be in bounds.
    pub fn cell(&self, coords: GridCoords) -> &CellType {
        &self.grid[self.bounds.index(coords)]
    }
    /// Returns mutable access to the entire cell at `coords`. Coordinates must be in bounds.
    pub fn cell_mut(&mut self, coords: GridCoords) -> &mut CellType {
        let index = self.bounds.index(coords);
        &mut self.grid[index]
    }
    /// Returns the entire cell at `coords`, or `None` outside the grid.
    pub fn get(&self, coords: GridCoords) -> Option<&CellType> {
        self.bounds.index_checked(coords).map(|index| &self.grid[index])
    }
    /// Iterates entire cells within `area` and the grid bounds, ignoring the area's altitude.
    pub fn cells_in(&self, area: GridArea) -> impl Iterator<Item = &CellType> {
        area.iter_in_bounds(self.bounds).map(|coords| self.cell(coords))
    }
}

impl<CellType> BaseGrid<CellType, GridVersion> where CellType: GridCell {
    /// Increments the version, wrapping on overflow.
    pub fn advance_version(&mut self) {
        self.version = self.version.wrapping_add(1);
    }
}

impl<CellType, GridVersionType> Index<GridVolumePosition> for BaseGrid<CellType, GridVersionType> where CellType: GridCell {
    type Output = CellType::Data;

    fn index(&self, position: GridVolumePosition) -> &Self::Output {
        self.cell(position.coords).at(position.level)
    }
}
impl<CellType, GridVersionType> IndexMut<GridVolumePosition> for BaseGrid<CellType, GridVersionType> where CellType: GridCell {
    fn index_mut(&mut self, position: GridVolumePosition) -> &mut Self::Output {
        self.cell_mut(position.coords).at_mut(position.level)
    }
}

impl<T, GridVersionType> Index<GridCoords> for BaseGrid<GridSingle<T>, GridVersionType> where T: Default + Clone {
    type Output = T;

    fn index(&self, coords: GridCoords) -> &T {
        self.cell(coords)
    }
}
impl<T, GridVersionType> IndexMut<GridCoords> for BaseGrid<GridSingle<T>, GridVersionType> where T: Default + Clone {
    fn index_mut(&mut self, coords: GridCoords) -> &mut T {
        self.cell_mut(coords)
    }
}
