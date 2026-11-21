//! # Blast propagation
//!
//! Resolves which cells and wisps an explosion reaches within a square radius (`reach` steps).
//!
//! ## Rules
//!
//! - **Origin level:** Detonates at the highest level in the landing cell that has a floor
//!   (`OnElevation` for walls and buildings, `OnGround` for ground and ramps, `Underground` for
//!   pits). Building tops count as a floor at `OnElevation`.
//! - **Propagation:** Breadth-first search up to `reach` steps in all 8 directions at the blast's
//!   fixed [`AltitudeLevel`]. Diagonal steps require at least one shared cardinal neighbor to be
//!   open (no corner-cutting).
//! - **Barriers:** Any cell occupied at the blast's level (walls, buildings, dark ore, solid
//!   earth, excavations) or outside map bounds blocks propagation (`BlastCell::Blocked`).
//! - **Floors and gaps:** Reached cells with a floor at the blast's level are `BlastCell::Hit` and
//!   damage wisps at that level. Reached cells without a floor (e.g. an open pit below `OnGround`,
//!   or open ground below `OnElevation`) are `BlastCell::Passed`: the blast crosses them without
//!   hitting.
//! - **Ramps:** Ramps provide a floor at the level above them (`OnGround`), matching wisp
//!   occupancy: an `OnGround` blast hits a ramp, while an `Underground` blast is blocked by the
//!   ramp structure.

use bevy::{ecs::system::SystemParam, prelude::*};
use strum::IntoEnumIterator;

use game_core::prelude::{AltitudeLevel, Bounds, EdgeSide, GridArea, GridCoords, GridImprint, GridPosition};
use grids::base::GridCell;
use indexes::{obstacles::ObstacleCell, prelude::ObstacleGrid, wisps::WispsGrid};

/// Maximum supported blast radius in grid steps; sizes the fixed stack buffers.
const MAX_REACH: u8 = 2;
const AREA_CELLS: usize = GridImprint::square_of_reach(MAX_REACH as i32).cell_count() as usize;

/// Resolves blast propagation and wisp hits against the obstacle and wisp grids.
#[derive(SystemParam)]
pub(crate) struct BlastHitManager<'w> {
    obstacle_grid: Res<'w, ObstacleGrid>,
    wisps_grid: Res<'w, WispsGrid>,
}
impl BlastHitManager<'_> {
    /// Detonates a blast at the highest floor in `coords` and spreads up to `reach` steps (at most
    /// [`MAX_REACH`]) at that level. Returns `None` if `coords` is out of bounds.
    pub(crate) fn go_off(&self, coords: GridCoords, reach: u8) -> Option<Blast<'_>> {
        let cell = self.obstacle_grid.get(coords)?;
        let level = AltitudeLevel::THREE_LAYERS.into_iter().rfind(|&level| has_blast_floor(cell, level)).unwrap_or(AltitudeLevel::Underground);
        Some(Blast::search(&self.obstacle_grid, &self.wisps_grid, coords.at(level), reach))
    }
}

/// Resolved blast propagation over the `(2 * reach + 1)²` area centered on `position`.
pub(crate) struct Blast<'a> {
    /// Origin cell and altitude level of the blast.
    pub(crate) position: GridPosition,
    reach: u8,
    /// Per-cell states in [`Self::area`] iteration order.
    cells: [BlastCell; AREA_CELLS],
    wisps_grid: &'a WispsGrid,
}
impl<'a> Blast<'a> {
    /// Breadth-first search from `position` in area-local coordinates, where `Bounds::index` maps
    /// each cell to its slot in `cells` and `BlastCell::OutOfReach` marks unvisited open cells.
    fn search(obstacle_grid: &ObstacleGrid, wisps_grid: &'a WispsGrid, position: GridPosition, reach: u8) -> Self {
        let level = position.level;
        let mut blast = Self { position, reach, cells: [BlastCell::Blocked; AREA_CELLS], wisps_grid };
        let area = blast.area();
        let bounds = Bounds::from(area.imprint);
        let mut open = [false; AREA_CELLS];
        let mut floor = [false; AREA_CELLS];
        for (index, coords) in area.iter().enumerate() {
            let cell = obstacle_grid.get(coords);
            open[index] = cell.is_some_and(|cell| cell.at(level).is_empty());
            floor[index] = cell.is_some_and(|cell| has_blast_floor(cell, level));
            if open[index] { blast.cells[index] = BlastCell::OutOfReach; }
        }
        let is_open = |local: GridCoords| bounds.contains(local) && open[bounds.index(local)];

        let mut queue = [(GridCoords::default(), 0); AREA_CELLS];
        let (mut head, mut tail) = (0, 0);
        let start = GridCoords::from((i32::from(reach), i32::from(reach)));
        if is_open(start) {
            let index = bounds.index(start);
            blast.cells[index] = BlastCell::reached(floor[index], 0);
            queue[tail] = (start, 0);
            tail += 1;
        }
        while head < tail {
            let (from, step) = queue[head];
            head += 1;
            if step == reach { continue; }
            let cardinals = EdgeSide::iter().map(|side| from.neighbor(side));
            let diagonals = EdgeSide::DIAGONALS.into_iter()
                .filter(|&(first, second)| is_open(from.neighbor(first)) || is_open(from.neighbor(second)))
                .map(|(first, second)| from.neighbor(first).neighbor(second));
            for target in cardinals.chain(diagonals) {
                if !bounds.contains(target) { continue; }
                let index = bounds.index(target);
                if blast.cells[index] != BlastCell::OutOfReach { continue; }
                blast.cells[index] = BlastCell::reached(floor[index], step + 1);
                queue[tail] = (target, step + 1);
                tail += 1;
            }
        }
        blast
    }

    fn area(&self) -> GridArea {
        GridArea::centered_on(self.position, GridImprint::square_of_reach(i32::from(self.reach)))
    }

    /// Iterates over all cells in the blast's square area and their [`BlastCell`] state.
    pub(crate) fn cells(&self) -> impl Iterator<Item = (GridCoords, BlastCell)> {
        self.area().iter().zip(self.cells.iter().copied())
    }

    /// Iterates over [`BlastCell::Hit`] cells and the wisps at the blast's level in each.
    pub(crate) fn hits(&self) -> impl Iterator<Item = BlastHit<'a>> {
        self.cells().filter_map(move |(coords, cell)| match cell {
            BlastCell::Hit { step } => {
                let position = coords.at(self.position.level);
                Some(BlastHit { position, step, wisps: self.wisps_grid.wisps_at(position) })
            }
            BlastCell::Blocked | BlastCell::OutOfReach | BlastCell::Passed { .. } => None,
        })
    }
}

/// A cell hit by a blast and the wisps occupying it at the blast's level.
pub(crate) struct BlastHit<'a> {
    pub(crate) position: GridPosition,
    /// Path distance in steps from the blast origin.
    pub(crate) step: u8,
    pub(crate) wisps: &'a [Entity],
}

/// Outcome of blast propagation for a single cell in the blast area.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum BlastCell {
    /// Blocked by a structure at the blast's level, or out of map bounds.
    Blocked,
    /// Open at the blast's level, but unreachable within `reach` steps.
    OutOfReach,
    /// Reached in `step` steps, but has no floor at the blast's level to hit.
    Passed { step: u8 },
    /// Reached in `step` steps and has a floor at the blast's level.
    Hit { step: u8 },
}
impl BlastCell {
    fn reached(has_floor: bool, step: u8) -> Self {
        if has_floor { Self::Hit { step } } else { Self::Passed { step } }
    }
}

/// Whether `cell` provides a floor for a blast at `level` (including building tops).
fn has_blast_floor(cell: &ObstacleCell, level: AltitudeLevel) -> bool {
    let below = cell.structure_under(level);
    below.provides_floor() || below.is_building()
}
