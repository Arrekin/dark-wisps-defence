use std::ops::{Deref, DerefMut, Index, IndexMut};

use bevy::prelude::*;
use game_core::prelude::{AltitudeLevel, Bounds, GridArea, GridCoords, GridPosition};

use crate::GridVersion;

/// Storage at one grid coordinate. Each cell type decides which value an altitude level reads.
pub trait GridCell {
    /// The value exposed at an altitude level.
    type Data;
    fn at(&self, level: AltitudeLevel) -> &Self::Data;
    fn at_mut(&mut self, level: AltitudeLevel) -> &mut Self::Data;
}

/// The levels a [`GridStack`] keeps a value for, from the lowest up.
pub const STACK_LEVELS: [AltitudeLevel; 3] = [AltitudeLevel::Underground, AltitudeLevel::OnGround, AltitudeLevel::OnElevation];

/// The index in [`STACK_LEVELS`] of the stack level that holds `level`.
/// `Airborne`, `AllEncompassing` and `TopLevel` belong to `OnElevation`.
pub const fn stack_slot(level: AltitudeLevel) -> usize {
    match level {
        AltitudeLevel::Underground => 0,
        AltitudeLevel::OnGround => 1,
        AltitudeLevel::OnElevation | AltitudeLevel::Airborne | AltitudeLevel::AllEncompassing | AltitudeLevel::TopLevel => 2,
    }
}
/// The stack level directly above the one holding `level`, or `None` at `OnElevation` and above.
pub const fn stack_level_above(level: AltitudeLevel) -> Option<AltitudeLevel> {
    let slot = stack_slot(level);
    if slot + 1 < STACK_LEVELS.len() { Some(STACK_LEVELS[slot + 1]) } else { None }
}
/// The stack level directly below the one holding `level`, or `None` at `Underground`.
pub const fn stack_level_below(level: AltitudeLevel) -> Option<AltitudeLevel> {
    let slot = stack_slot(level);
    if slot > 0 { Some(STACK_LEVELS[slot - 1]) } else { None }
}

/// One value per level in [`STACK_LEVELS`].
pub type GridStack<T> = [T; STACK_LEVELS.len()];
impl<T> GridCell for GridStack<T> {
    type Data = T;
    fn at(&self, level: AltitudeLevel) -> &T { &self[stack_slot(level)] }
    fn at_mut(&mut self, level: AltitudeLevel) -> &mut T { &mut self[stack_slot(level)] }
}

/// One value per grid coordinate.
#[derive(Default, Clone, Debug)]
pub struct GridSingle<T>(pub T);
impl<T> Deref for GridSingle<T> {
    type Target = T;
    fn deref(&self) -> &T { &self.0 }
}
impl<T> DerefMut for GridSingle<T> {
    fn deref_mut(&mut self) -> &mut T { &mut self.0 }
}

/// A bounded grid with one cell per coordinate.
///
/// Indexing by [`GridPosition`] selects a value within a [`GridCell`]. Use [`Self::cell`]
/// to access the entire cell. Grids of [`GridSingle`] also support indexing by [`GridCoords`].
#[derive(Resource, Default)]
pub struct BaseGrid<CellType, GridVersionType> {
    pub bounds: Bounds,
    pub grid: Vec<CellType>,
    /// Caller-managed counter: a change version for indexes or a generation stamp for searches.
    pub version: GridVersionType,
}

impl<CellType, GridVersionType> BaseGrid<CellType, GridVersionType> where CellType: Default + Clone {
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
    /// Copies the bounds and cells of `source`, reusing this grid's allocation.
    /// The version stays as it was.
    pub fn copy_from(&mut self, source: &Self) {
        self.bounds = source.bounds;
        self.grid.clone_from(&source.grid);
    }
}

impl<CellType, GridVersionType> BaseGrid<CellType, GridVersionType> {
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

impl<CellType> BaseGrid<CellType, GridVersion> {
    /// Increments the version, wrapping on overflow.
    pub fn advance_version(&mut self) {
        self.version = self.version.wrapping_add(1);
    }
}

impl<CellType, GridVersionType> Index<GridPosition> for BaseGrid<CellType, GridVersionType> where CellType: GridCell {
    type Output = CellType::Data;

    fn index(&self, position: GridPosition) -> &Self::Output {
        self.cell(position.coords).at(position.level)
    }
}
impl<CellType, GridVersionType> IndexMut<GridPosition> for BaseGrid<CellType, GridVersionType> where CellType: GridCell {
    fn index_mut(&mut self, position: GridPosition) -> &mut Self::Output {
        self.cell_mut(position.coords).at_mut(position.level)
    }
}

impl<T, GridVersionType> Index<GridCoords> for BaseGrid<GridSingle<T>, GridVersionType> {
    type Output = T;

    fn index(&self, coords: GridCoords) -> &T {
        self.cell(coords)
    }
}
impl<T, GridVersionType> IndexMut<GridCoords> for BaseGrid<GridSingle<T>, GridVersionType> {
    fn index_mut(&mut self, coords: GridCoords) -> &mut T {
        self.cell_mut(coords)
    }
}
