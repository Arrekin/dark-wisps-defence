use std::collections::VecDeque;

use game_core::prelude::{Bounds, GridCoords};

use crate::{CellTrait, base::BaseGrid};

/// A counter used to mark cells visited during a search.
pub trait Stamp: Copy + PartialEq {
    /// Initial generation stamp; must differ from the stamp in a default cell.
    const FIRST: Self;
    /// Returns the next stamp, or `None` on overflow.
    fn checked_next(self) -> Option<Self>;
}
impl Stamp for u8 {
    const FIRST: Self = 1;
    fn checked_next(self) -> Option<Self> { self.checked_add(1) }
}
impl Stamp for u32 {
    const FIRST: Self = 1;
    fn checked_next(self) -> Option<Self> { self.checked_add(1) }
}

/// The current search's stamp, compared against cell stamps to identify visited cells.
///
/// The `u8` and `u32` implementations start at 1, leaving default cells unvisited.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Generation<S>(S);
impl<S: Stamp> Generation<S> {
    /// Stamp written to cells visited in this generation.
    pub fn stamp(self) -> S {
        self.0
    }
}
impl<S: Stamp> Default for Generation<S> {
    fn default() -> Self { Self(S::FIRST) }
}

impl<C: CellTrait, S: Stamp> BaseGrid<C, Generation<S>> {
    pub fn resize_and_clear(&mut self, bounds: impl Into<Bounds>) {
        self.resize(bounds);
        self.clear();
    }
    /// Starts a new search by advancing the generation, making previous visits obsolete.
    /// Resets cell storage only when the generation counter overflows.
    pub fn clear(&mut self) {
        match self.version.0.checked_next() {
            Some(next) => self.version = Generation(next),
            None => {
                self.reset();
                self.version = Generation::default();
            }
        }
    }
}

pub type VisitedGrid = BaseGrid<u8, Generation<u8>>;
impl VisitedGrid {
    pub fn set_visited(&mut self, coords: GridCoords) {
        self[coords] = self.version.stamp();
    }
    pub fn is_visited(&self, coords: GridCoords) -> bool {
        self[coords] == self.version.stamp()
    }
}

/// A visited cell's generation stamp and predecessor, used to reconstruct a path.
#[derive(Copy, Clone, Debug, PartialEq, Default)]
pub struct Tracking {
    pub stamp: u32,
    pub from: GridCoords,
}

pub type TrackingGrid = BaseGrid<Tracking, Generation<u32>>;
impl TrackingGrid {
    pub fn set_tracked(&mut self, coords: GridCoords, from: GridCoords) {
        self[coords] = Tracking { stamp: self.version.stamp(), from };
    }
    pub fn is_tracked(&self, coords: GridCoords) -> bool {
        self[coords].stamp == self.version.stamp()
    }
    /// Replaces `path` with the route from `start` to `end`, excluding `start` and including `end`.
    /// The route is reconstructed by following predecessors from `end`.
    pub fn compile_path_into(&self, start: GridCoords, end: GridCoords, path: &mut VecDeque<GridCoords>) {
        path.clear();
        let mut current = end;
        while current != start {
            assert!(self.is_tracked(current), "Encountered untracked cell during compilation of path");
            path.push_front(current);
            current = self[current].from;
        }
    }
}
