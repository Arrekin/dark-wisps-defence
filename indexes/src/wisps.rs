use std::{ops::Deref, slice};

use bevy::prelude::*;

use game_core::prelude::{AltitudeLevel, Bounds, GridCoords, GridPosition, TargetLevels};
use grids::{GridVersion, base::{BaseGrid, GridCell, GridStack}};

use crate::obstacles::ObstacleGrid;

/// Tracks wisps by coordinate and layer. Membership changes advance the grid version.
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

    /// Returns the wisps in the layer holding `position`.
    ///
    /// Coordinates must be in bounds.
    pub fn wisps_at(&self, position: GridPosition) -> &[Entity] {
        &self.cells[position]
    }

    /// Iterates wisps at `coords` across all layers.
    ///
    /// Coordinates must be in bounds.
    pub fn wisps_in_cell(&self, coords: GridCoords) -> impl Iterator<Item = Entity> {
        self.cell(coords).iter().flatten().copied()
    }

    /// Iterates wisps at `coords` matching the `levels` filter.
    ///
    /// Coordinates must be in bounds.
    pub fn targetable_wisps(&self, obstacle_grid: &ObstacleGrid, levels: TargetLevels, coords: GridCoords) -> impl Iterator<Item = Entity> {
        let stack = self.cell(coords);
        let allowed = match levels {
            TargetLevels::Tops => slice::from_ref(stack.at(obstacle_grid.cell(coords).top_level())),
            TargetLevels::EveryLevel => stack.as_slice(),
        };
        allowed.iter().flatten().copied()
    }

    pub fn wisp_add(&mut self, position: GridPosition, wisp: Entity) {
        self.cells[position].push(wisp);
        self.cells.advance_version();
    }

    pub fn wisp_remove(&mut self, position: GridPosition, wisp: Entity) {
        let cell = &mut self.cells[position];
        let Some(index) = cell.iter().position(|&listed| listed == wisp) else { return; };
        cell.swap_remove(index);
        self.cells.advance_version();
    }

    pub fn wisp_move(&mut self, from: GridPosition, to: GridPosition, wisp: Entity) {
        self.wisp_remove(from, wisp);
        self.wisp_add(to, wisp);
    }

    /// Removes and despawns wisps at `coords` on `lowest` and all levels above it.
    ///
    /// Coordinates must be in bounds.
    pub fn despawn_from_level_upwards(&mut self, commands: &mut Commands, coords: GridCoords, lowest: AltitudeLevel) {
        let mut removed_any = false;
        for wisps in &mut self.cells.cell_mut(coords)[lowest.three_layer_index()..] {
            removed_any |= !wisps.is_empty();
            for wisp in wisps.drain(..) {
                commands.entity(wisp).despawn();
            }
        }
        if removed_any { self.cells.advance_version(); }
    }
}
