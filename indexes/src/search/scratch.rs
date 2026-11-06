//! Reusable search grids and queues, shared by searches on the same thread to retain allocations.

use std::{cell::RefCell, cmp::Ordering, collections::{BinaryHeap, VecDeque}};

use game_core::prelude::{GridCoords, GridVolumePosition};
use grids::{base::{GridSingle, GridStack}, visited::{TrackingGrid, VisitedGrid}};

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
    /// Visit stamps per coordinate, shared across altitude levels.
    pub coords_visited: VisitedGrid<GridSingle<u8>>,
    /// Visit stamps per coordinate and volume level.
    pub position_visited: VisitedGrid<GridStack<u8>>,
    /// Best-first queue ordered by floating-point cost.
    pub cost_queue: BinaryHeap<State<f32, GridVolumePosition>>,
    /// Best-first queue ordered by step count.
    pub step_queue: BinaryHeap<State<usize, GridCoords>>,
    /// FIFO queue of cells and distances for range-limited floods.
    pub coords_distance_queue: VecDeque<(usize, GridCoords)>,
    /// FIFO queue of positions and distances for range-limited floods.
    pub position_distance_queue: VecDeque<(usize, GridVolumePosition)>,
    /// FIFO queue of cells for floods without a range limit.
    pub coords_queue: VecDeque<GridCoords>,
}

#[derive(Copy, Clone, Debug, PartialEq)]
pub struct State<T, Key> {
    pub cost: T,
    pub distance: usize,
    pub key: Key,
}
impl<T, Key> Ord for State<T, Key> where T: PartialOrd, Key: PartialEq {
    fn cmp(&self, other: &Self) -> Ordering {
        // Reverse the comparison so BinaryHeap pops the lowest cost first.
        other.cost.partial_cmp(&self.cost).unwrap_or(Ordering::Equal)
    }
}
impl<T, Key> PartialOrd for State<T, Key> where T: PartialOrd, Key: PartialEq {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}
impl<T, Key> Eq for State<T, Key> where T: PartialOrd, Key: PartialEq {}
