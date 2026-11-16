//! # Grid movement rules
//!
//! Pathfinding and emission floods use the same floor support, surface style, ramp and tunnel rules.
//! Reading shapes through [`GridStructure`] applies these rules to both walls at `OnGround` and
//! excavations at `Underground`.
//!
//! Positions on ramps use the level above the ramp, even at the low end. Positions inside tunnels
//! use the tunnel's level. Movement into or out of a tunnel must cross an open cardinal side;
//! diagonal steps cannot enter or leave a tunnel.

use strum::IntoEnumIterator;

use game_core::prelude::{AltitudeLevel, EdgeSide, GridPosition};
use grids::base::{GridCell, stack_level_above, stack_level_below};
use map_objects::wall_style::WallStyleKey;

use crate::obstacles::{GridStructure, ObstacleCell, ObstacleGrid};

/// Returns reachable cardinal neighbors, including level changes through ramps.
///
/// Visits sides in [`EdgeSide`] order, which can affect which route a search finds first.
/// The starting coordinates must be in bounds.
pub fn walkable_neighbors(obstacle_grid: &ObstacleGrid, position: GridPosition) -> impl Iterator<Item = GridPosition> {
    let from_cell = obstacle_grid.cell(position.coords);
    let from_occupant = *from_cell.at(position.level);
    let from_support_below = from_cell.structure_under(position.level);
    EdgeSide::iter().filter_map(move |side| {
        let coords = position.coords.neighbor(side);
        let target_cell = obstacle_grid.get(coords)?;
        level_after_step(position.level, from_occupant, from_support_below, side, target_cell).map(|level| coords.at(level))
    })
}

/// Returns reachable diagonal neighbors in [`EdgeSide::DIAGONALS`] order.
///
/// Diagonal moves stay on the same level and cannot enter or leave ramps or tunnels. Both cells
/// beside the diagonal must be empty, with floor support and a compatible surface style.
/// The starting coordinates must be in bounds.
pub fn walkable_diagonals(obstacle_grid: &ObstacleGrid, position: GridPosition) -> impl Iterator<Item = GridPosition> {
    let level = position.level;
    let from_cell = obstacle_grid.cell(position.coords);
    let from_support_below = from_cell.structure_under(level);
    let is_inside_tunnel = blocks_movement(*from_cell.at(level));
    let from_style = from_support_below.style();
    let diagonals: &[_] = if from_support_below.is_ramp() || is_inside_tunnel { &[] } else { &EdgeSide::DIAGONALS };
    diagonals.iter().filter_map(move |&(first_side, second_side)| {
        let first_coords = position.coords.neighbor(first_side);
        let second_coords = position.coords.neighbor(second_side);
        let target_coords = first_coords.neighbor(second_side);
        let target_cell = obstacle_grid.get(target_coords)?;

        let is_open = |cell: &ObstacleCell| cell.at(level).is_empty() && is_flat_surface(cell, level, from_style);
        let can_move = is_flat_surface(target_cell, level, from_style) && is_open(obstacle_grid.cell(first_coords)) && is_open(obstacle_grid.cell(second_coords));
        can_move.then(|| target_coords.at(level))
    })
}

/// Returns the landing level for a step across `side`, or `None` if blocked.
/// `from_occupant` occupies the starting level; `from_support_below` lies directly beneath it.
fn level_after_step(from_level: AltitudeLevel, from_occupant: GridStructure, from_support_below: GridStructure, side: EdgeSide, target_cell: &ObstacleCell) -> Option<AltitudeLevel> {
    // Leaving a tunnel requires an opening on the exit side.
    if blocks_movement(from_occupant) && !from_occupant.openings().contains(side) {
        return None;
    }

    let mut landing = from_level;

    // A ramp exit descends at the low edge and stays level at the high edge.
    if let Some(high_side) = from_support_below.ramp_high_side() {
        if side == high_side.opposite() {
            landing = stack_level_below(landing)?;
        } else if side != high_side {
            return None;
        }
    }

    // Enter a ramp at the landing level through its low edge, then use the level above it.
    if let Some(high_side) = target_cell.at(landing).ramp_high_side() {
        if side != high_side {
            return None;
        }
        landing = stack_level_above(landing)?;
    // A ramp below the landing level can only be entered through its high edge.
    } else if let Some(high_side) = target_cell.structure_under(landing).ramp_high_side() && side != high_side.opposite() {
        return None;
    }

    let can_land = can_enter_across(*target_cell.at(landing), side) && has_joining_floor(target_cell, landing, from_support_below.style());
    can_land.then_some(landing)
}

/// Checks whether a step in direction `side` can enter the target cell's level.
/// Entering a blocking structure requires an opening on the side facing the starting cell.
fn can_enter_across(structure: GridStructure, side: EdgeSide) -> bool {
    !blocks_movement(structure) || structure.openings().contains(side.opposite())
}

/// Checks for a passable level with floor support, no ramp, and a compatible surface style.
fn is_flat_surface(cell: &ObstacleCell, level: AltitudeLevel, from_style: Option<WallStyleKey>) -> bool {
    !cell.structure_under(level).is_ramp() && !blocks_movement(*cell.at(level)) && has_joining_floor(cell, level, from_style)
}

/// Checks for floor support beneath `level` and a surface style compatible with `from_style`.
fn has_joining_floor(cell: &ObstacleCell, level: AltitudeLevel, from_style: Option<WallStyleKey>) -> bool {
    cell.has_floor(level) && styles_join(from_style, cell.structure_under(level).style())
}

/// Identifies structures that need an opening check before entry or exit.
/// Buildings remain passable so paths can reach attack targets.
fn blocks_movement(structure: GridStructure) -> bool {
    match structure {
        GridStructure::Solid | GridStructure::DarkOre { .. } | GridStructure::Wall { .. } | GridStructure::Excavation { .. } => true,
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
