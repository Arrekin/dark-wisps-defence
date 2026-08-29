use bevy::prelude::*;

use game_core::prelude::GridCoords;

use crate::{GridVersion, base::BaseGrid};

pub type VisitedGrid = BaseGrid<bool, GridVersion>;
impl VisitedGrid {
    pub fn set_visited(&mut self, coords: GridCoords) {
        let index = self.index(coords);
        self.grid[index] = true;
    }
    pub fn is_visited(&self, coords: GridCoords) -> bool {
        let index = self.index(coords);
        self.grid[index]
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
        let index = self.index(coords);
        self.grid[index] = Tracking::Tracked {from};
    }
    pub fn is_tracked(&self, coords: GridCoords) -> bool {
        let index = self.index(coords);
        self.grid[index] != Tracking::Unvisited
    }
    // Creates a path between given points.
    // The path is created in reverse(backtracking "from" into "to") as during BFS we are saving from which field we came.
    pub fn compile_path(&self, from_coords: GridCoords, to_coords: GridCoords) -> Vec<GridCoords> {
        let mut path = vec![from_coords];
        let mut curr_coords = from_coords;
        loop {
            curr_coords = match self.grid[self.index(curr_coords)] {
                Tracking::Unvisited => panic!("Encountered unvisited field during compilation of path"),
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
