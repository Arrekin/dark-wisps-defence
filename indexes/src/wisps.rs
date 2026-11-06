use std::ops::Deref;

use bevy::prelude::*;

use game_core::prelude::{Bounds, GridCoords, GridVolumePosition};
use grids::{GridVersion, base::{BaseGrid, GridStack}};

/// Tracks wisps by coordinate and volume level. Membership changes advance the grid version.
#[derive(Resource, Default)]
pub struct WispsGrid {
    cells: BaseGrid<GridStack<Vec<Entity>>, GridVersion>,
}
impl Deref for WispsGrid {
    type Target = BaseGrid<GridStack<Vec<Entity>>, GridVersion>;
    fn deref(&self) -> &Self::Target { &self.cells }
}

impl WispsGrid {
    pub fn new_with_size(bounds: impl Into<Bounds>) -> Self {
        Self { cells: BaseGrid::new_with_size(bounds) }
    }

    /// Returns the wisps in one cell volume.
    ///
    /// Coordinates must be in bounds.
    pub fn wisps_at(&self, position: GridVolumePosition) -> &[Entity] {
        &self.cells[position]
    }

    /// Iterates wisps at `coords` across all volume levels.
    ///
    /// Coordinates must be in bounds.
    pub fn wisps_in_cell(&self, coords: GridCoords) -> impl Iterator<Item = Entity> {
        self.cell(coords).iter().flatten().copied()
    }

    pub fn wisp_add(&mut self, position: GridVolumePosition, wisp: Entity) {
        self.cells[position].push(wisp);
        self.cells.advance_version();
    }

    pub fn wisp_remove(&mut self, position: GridVolumePosition, wisp: Entity) {
        let cell = &mut self.cells[position];
        let Some(index) = cell.iter().position(|&listed| listed == wisp) else { return; };
        cell.swap_remove(index);
        self.cells.advance_version();
    }

    pub fn wisp_move(&mut self, from: GridVolumePosition, to: GridVolumePosition, wisp: Entity) {
        self.wisp_remove(from, wisp);
        self.wisp_add(to, wisp);
    }
}
