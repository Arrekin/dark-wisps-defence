use std::{cell::RefCell, cmp::Ordering};

use game_core::prelude::{Bounds, GridCoords};

use crate::visited::{TrackingGrid, VisitedGrid};

thread_local! {
    pub static TRACKING_GRID: RefCell<TrackingGrid> = RefCell::new(TrackingGrid::new_with_size(Bounds::default()));
    pub static VISITED_GRID: RefCell<VisitedGrid> = RefCell::new(VisitedGrid::new_with_size(Bounds::default()));
}

#[derive(Copy, Clone, Debug, PartialEq)]
pub struct State<T> where T: PartialOrd {
    pub cost: T,
    pub distance: usize,
    pub coords: GridCoords,
}
impl<T> Ord for State<T> where T: PartialOrd {
    fn cmp(&self, other: &Self) -> Ordering {
        // Reversed: lower cost sorts as "greater" so BinaryHeap (a max-heap)
        // surfaces the lowest-cost state at the top.
        other.cost.partial_cmp(&self.cost).unwrap_or(Ordering::Equal)
    }
}
impl<T> PartialOrd for State<T> where T: PartialOrd {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}
impl<T> Eq for State<T> where T: PartialOrd {}
