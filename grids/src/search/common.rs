use std::{cell::RefCell, cmp::Ordering};

use game_core::prelude::GridCoords;

use crate::visited::{TrackingGrid, VisitedGrid};

pub const CARDINAL_DIRECTIONS: [(i32, i32); 4] = [(0, 1), (1, 0), (0, -1), (-1, 0)];
pub const ALL_DIRECTIONS: [(i32, i32); 8] = [
    (0, 1), (1, 0), (0, -1), (-1, 0), // Cardinal directions
    (1, 1), (1, -1), (-1, 1), (-1, -1) // Diagonal directions
];

thread_local! {
    pub static TRACKING_GRID: RefCell<TrackingGrid> = RefCell::new(TrackingGrid::new_with_size(0, 0));
    pub static VISITED_GRID: RefCell<VisitedGrid> = RefCell::new(VisitedGrid::new_with_size(0, 0));
}

#[derive(Copy, Clone, Debug, PartialEq)]
pub struct State<T> where T: PartialOrd {
    pub cost: T,
    pub distance: usize,
    pub coords: GridCoords,
}
impl<T> PartialOrd for State<T> where T: PartialOrd {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        other.cost.partial_cmp(&self.cost)
    }
}
impl<T> Ord for State<T> where T: PartialOrd {
    fn cmp(&self, other: &Self) -> Ordering {
        other.partial_cmp(&self).unwrap()
    }
}
impl<T> Eq for State<T> where T: PartialOrd {}
