//! # Grid movement rules
//!
//! Pathfinding and emission floods share the same rules for floor support, wall styles, ramps,
//! and tunnels.
//!
//! Positions on ramps use the volume above the ramp, even at the low end. Positions inside tunnels
//! use the wall's volume. Movement into or out of a tunnel must cross an open cardinal side;
//! diagonal steps cannot enter or leave a tunnel.

use strum::IntoEnumIterator;

use game_core::prelude::{EdgeSide, GridVolumePosition, VolumeLevel};
use grids::base::GridCell;
use map_objects::{prelude::WallShape, wall_style::WallStyleKey};

use crate::obstacles::{GridStructure, ObstacleCell, ObstacleGrid};

/// Returns reachable cardinal neighbors, including level changes through ramps.
///
/// Visits sides in [`EdgeSide`] order, which can affect which route a search finds first.
/// The starting coordinates must be in bounds.
pub fn walkable_neighbors(obstacle_grid: &ObstacleGrid, position: GridVolumePosition) -> impl Iterator<Item = GridVolumePosition> {
    let from_cell = obstacle_grid.cell(position.coords);
    let from_occupant = *from_cell.at(position.level);
    let from_support_below = structure_under(from_cell, position.level);
    EdgeSide::iter().filter_map(move |side| {
        let coords = position.coords.neighbor(side);
        let target_cell = obstacle_grid.get(coords)?;
        level_after_step(position.level, from_occupant, from_support_below, side, target_cell).map(|level| coords.at(level))
    })
}

/// Returns reachable diagonal neighbors in [`EdgeSide::DIAGONALS`] order.
///
/// Diagonal moves stay on the same level and cannot enter or leave ramps or tunnels. Both cells
/// beside the diagonal must be empty, with floor support and a compatible wall style.
/// The starting coordinates must be in bounds.
pub fn walkable_diagonals(obstacle_grid: &ObstacleGrid, position: GridVolumePosition) -> impl Iterator<Item = GridVolumePosition> {
    let level = position.level;
    let from_cell = obstacle_grid.cell(position.coords);
    let from_support_below = structure_under(from_cell, level);
    let is_inside_wall = from_cell.at(level).wall_shape().is_some();
    let from_style = from_support_below.style();
    let diagonals: &[_] = if from_support_below.is_ramp() || is_inside_wall { &[] } else { &EdgeSide::DIAGONALS };
    diagonals.iter().filter_map(move |&(first_side, second_side)| {
        let first_coords = position.coords.neighbor(first_side);
        let second_coords = position.coords.neighbor(second_side);
        let target_coords = first_coords.neighbor(second_side);
        let target_cell = obstacle_grid.get(target_coords)?;

        let is_open = |cell: &ObstacleCell| *cell.at(level) == GridStructure::Empty && is_flat_surface(cell, level, from_style);
        let can_move = is_flat_surface(target_cell, level, from_style) && is_open(obstacle_grid.cell(first_coords)) && is_open(obstacle_grid.cell(second_coords));
        can_move.then(|| target_coords.at(level))
    })
}

/// Returns the landing level for a step across `side`, or `None` if blocked.
/// `from_occupant` occupies the starting volume; `from_support_below` lies directly beneath it.
fn level_after_step(from_level: VolumeLevel, from_occupant: GridStructure, from_support_below: GridStructure, side: EdgeSide, target_cell: &ObstacleCell) -> Option<VolumeLevel> {
    // Leaving a wall requires an opening on the exit side.
    if from_occupant.wall_shape().is_some_and(|shape| !shape.is_open_toward(side)) {
        return None;
    }

    let mut landing = from_level;

    // A ramp exit descends at the low edge and stays level at the high edge.
    if let Some(WallShape::Ramp { high_side }) = from_support_below.wall_shape() {
        if side == high_side.opposite() {
            landing = landing.below()?;
        } else if side != high_side {
            return None;
        }
    }

    // Enter a ramp in the landing volume through its low edge, then use the volume above it.
    if let Some(WallShape::Ramp { high_side }) = target_cell.at(landing).wall_shape() {
        if side != high_side {
            return None;
        }
        landing = landing.above()?;
    // A ramp below the landing volume can only be entered through its high edge.
    } else if let Some(WallShape::Ramp { high_side }) = structure_under(target_cell, landing).wall_shape() && side != high_side.opposite() {
        return None;
    }

    let can_land = can_enter_across(*target_cell.at(landing), side) && has_joining_floor(target_cell, landing, from_support_below.style());
    can_land.then_some(landing)
}

/// Checks whether a step in direction `side` can enter the target volume.
/// Entering a wall requires an opening on the side facing the starting cell.
fn can_enter_across(structure: GridStructure, side: EdgeSide) -> bool {
    match structure.wall_shape() {
        Some(shape) => shape.is_open_toward(side.opposite()),
        None => !blocks_movement(structure),
    }
}

/// Checks for a passable volume with floor support, no ramp, and a compatible surface style.
fn is_flat_surface(cell: &ObstacleCell, level: VolumeLevel, from_style: Option<WallStyleKey>) -> bool {
    !structure_under(cell, level).is_ramp() && !blocks_movement(*cell.at(level)) && has_joining_floor(cell, level, from_style)
}

/// Checks for floor support beneath `level` and a surface style compatible with `from_style`.
fn has_joining_floor(cell: &ObstacleCell, level: VolumeLevel, from_style: Option<WallStyleKey>) -> bool {
    cell.has_floor(level.floor()) && styles_join(from_style, structure_under(cell, level).style())
}

/// Solid earth, ore, and walls block entry into their volume.
/// Buildings allow entry so a path can reach an attack target.
fn blocks_movement(structure: GridStructure) -> bool {
    match structure {
        GridStructure::Solid | GridStructure::DarkOre { .. } | GridStructure::Wall { .. } => true,
        GridStructure::Empty | GridStructure::Building { .. } => false,
    }
}

/// Two styled surfaces connect only when their styles match.
/// An unstyled surface connects to any style.
fn styles_join(first: Option<WallStyleKey>, second: Option<WallStyleKey>) -> bool {
    match (first, second) {
        (Some(first), Some(second)) => first == second,
        _ => true,
    }
}

/// The structure in the volume below `level`. Solid earth lies below the lowest level.
fn structure_under(cell: &ObstacleCell, level: VolumeLevel) -> GridStructure {
    level.below().map_or(GridStructure::Solid, |below| *cell.at(below))
}
