use bevy::prelude::*;

use game_core::prelude::{Bounds, GridCoords};

use crate::{GridVersion, base::BaseGrid};

/// Stamp that marks a cell as visited in the current search. Starts at 1, so zeroed cells read as unvisited.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct VisitedGeneration(u8);
impl VisitedGeneration {
    /// The generation after this one. `None` when the counter would wrap.
    pub fn advanced(self) -> Option<Self> {
        self.0.checked_add(1).map(Self)
    }
    /// The value a cell holds once visited in this generation.
    pub fn stamp(self) -> u8 {
        self.0
    }
}
impl Default for VisitedGeneration {
    fn default() -> Self { Self(1) }
}

/// Visited cells carry the generation of the search that visited them, so clearing is one increment.
pub type VisitedGrid = BaseGrid<u8, VisitedGeneration>;
impl VisitedGrid {
    pub fn resize_and_clear(&mut self, bounds: impl Into<Bounds>) {
        self.resize(bounds);
        self.clear();
    }
    /// Marks every cell unvisited. Resets the cells only when the generation wraps.
    pub fn clear(&mut self) {
        match self.version.advanced() {
            Some(next) => self.version = next,
            None => {
                self.reset();
                self.version = VisitedGeneration::default();
            }
        }
    }
    pub fn set_visited(&mut self, coords: GridCoords) {
        self[coords] = self.version.stamp();
    }
    pub fn is_visited(&self, coords: GridCoords) -> bool {
        self[coords] == self.version.stamp()
    }
}


// Grid that keeps track of visited fields during BFS search so it is possible to backtrack.
#[derive(Copy, Clone, Debug, PartialEq, Default)]
pub enum Tracking {
    #[default]
    Unvisited,
    Tracked {from: GridCoords},
}


pub type TrackingGrid = BaseGrid<Tracking, GridVersion>;
impl TrackingGrid {
    pub fn set_tracked(&mut self, coords: GridCoords, from: GridCoords) {
        self[coords] = Tracking::Tracked {from};
    }
    pub fn is_tracked(&self, coords: GridCoords) -> bool {
        self[coords] != Tracking::Unvisited
    }
    // Creates a path between given points.
    // The path is created in reverse(backtracking "from" into "to") as during BFS we are saving from which field we came.
    pub fn compile_path(&self, from_coords: GridCoords, to_coords: GridCoords) -> Vec<GridCoords> {
        let mut path = vec![from_coords];
        let mut curr_coords = from_coords;
        loop {
            curr_coords = match self[curr_coords] {
                Tracking::Unvisited => panic!("Encountered unvisited cell during compilation of path"),
                Tracking::Tracked {from} => from,
            };
            if curr_coords != to_coords {
                path.push(curr_coords);
            } else { break; }
        }
        path.reverse();
        path
    }
}
