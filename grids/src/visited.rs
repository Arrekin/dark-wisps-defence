use std::{collections::VecDeque, ops::{Index, IndexMut}};

use game_core::prelude::{Bounds, GridVolumePosition, VolumeLevel};

use crate::base::{BaseGrid, GridCell, GridStack};

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

impl<CellType: GridCell, S: Stamp> BaseGrid<CellType, Generation<S>> {
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

/// Tracks visits in the current search using generation stamps.
///
/// Use [`crate::base::GridSingle`] for visits per coordinate or [`GridStack`] for visits
/// per volume position.
pub type VisitedGrid<CellType> = BaseGrid<CellType, Generation<u8>>;
impl<CellType: GridCell<Data = u8>> VisitedGrid<CellType> {
    pub fn set_visited<Key>(&mut self, key: Key) where Self: IndexMut<Key, Output = u8> {
        self[key] = self.version.stamp();
    }
    pub fn is_visited<Key>(&self, key: Key) -> bool where Self: Index<Key, Output = u8> {
        self[key] == self.version.stamp()
    }
}

/// A generation stamp and predecessor offset for reconstructing a path.
#[derive(Copy, Clone, Debug, PartialEq, Default)]
pub struct Tracking {
    stamp: u32,
    /// Coordinate offset from this position to its predecessor.
    step_back: (i8, i8),
    from_level: VolumeLevel,
}

const _: () = assert!(size_of::<Tracking>() == 8, "Tracking is written for every position a path search reaches");

pub type TrackingGrid = BaseGrid<GridStack<Tracking>, Generation<u32>>;
impl TrackingGrid {
    /// Marks `position` as visited and records `from` as its predecessor.
    /// The coordinates must differ by at most one cell on each axis.
    pub fn set_tracked(&mut self, position: GridVolumePosition, from: GridVolumePosition) {
        let step_back = ((from.coords.x - position.coords.x) as i8, (from.coords.y - position.coords.y) as i8);
        self[position] = Tracking { stamp: self.version.stamp(), step_back, from_level: from.level };
    }
    pub fn is_tracked(&self, position: GridVolumePosition) -> bool {
        self[position].stamp == self.version.stamp()
    }
    /// Replaces `path` with the route from `start` to `end`, excluding `start` and including `end`.
    pub fn compile_path_into(&self, start: GridVolumePosition, end: GridVolumePosition, path: &mut VecDeque<GridVolumePosition>) {
        path.clear();
        let mut current = end;
        while current != start {
            assert!(self.is_tracked(current), "Encountered untracked position during compilation of path");
            path.push_front(current);
            let Tracking { step_back: (step_x, step_y), from_level, .. } = self[current];
            current = current.coords.shifted((step_x as i32, step_y as i32)).at(from_level);
        }
    }
}
