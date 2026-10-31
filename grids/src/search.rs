//! Reusable search grids and queues, shared by searches on the same thread to retain allocations.

use std::{cell::RefCell, cmp::Ordering, collections::{BinaryHeap, VecDeque}};

use game_core::prelude::GridCoords;

use crate::visited::{TrackingGrid, VisitedGrid};

thread_local! {
    /// Shared scratch storage. Starting another search while this is borrowed mutably panics.
    pub static SEARCH_SCRATCH: RefCell<SearchScratch> = RefCell::new(SearchScratch::default());
}

/// Reusable grids and queues for searches.
///
/// Each search must clear the grid and queue it uses before starting. A previous search may
/// have returned early with entries still in its queue. Clearing retains the allocations.
#[derive(Default)]
pub struct SearchScratch {
    /// For searches that backtrack their path.
    pub tracking: TrackingGrid,
    /// For searches that only mark visited cells.
    pub visited: VisitedGrid,
    /// Best-first queue ordered by floating-point cost.
    pub cost_queue: BinaryHeap<State<f32>>,
    /// Best-first queue ordered by step count.
    pub step_queue: BinaryHeap<State<usize>>,
    /// FIFO queue of cells and distances for range-limited floods.
    pub distance_queue: VecDeque<(usize, GridCoords)>,
    /// FIFO queue of cells for floods without a range limit.
    pub coords_queue: VecDeque<GridCoords>,
}

#[derive(Copy, Clone, Debug, PartialEq)]
pub struct State<T> where T: PartialOrd {
    pub cost: T,
    pub distance: usize,
    pub coords: GridCoords,
}
impl<T> Ord for State<T> where T: PartialOrd {
    fn cmp(&self, other: &Self) -> Ordering {
        // Reverse the comparison so BinaryHeap pops the lowest cost first.
        other.cost.partial_cmp(&self.cost).unwrap_or(Ordering::Equal)
    }
}
impl<T> PartialOrd for State<T> where T: PartialOrd {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}
impl<T> Eq for State<T> where T: PartialOrd {}
