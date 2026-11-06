//! # Grid movement rules
//!
//! Pathfinding and emission floods share these rules for floor support, wall styles, and ramps.
//! A position on a ramp uses the volume above the ramp, including at its low end.

use strum::IntoEnumIterator;

use game_core::prelude::{EdgeSide, GridVolumePosition, VolumeLevel};
use grids::base::GridCell;
use map_objects::wall_style::WallStyleKey;

use crate::obstacles::{GridStructure, ObstacleCell, ObstacleGrid};

/// Returns reachable cardinal neighbors, including level changes through ramps.
///
/// Visits sides in [`EdgeSide`] order, which can affect which route a search finds first.
/// The starting coordinates must be in bounds.
pub fn walkable_neighbors(obstacle_grid: &ObstacleGrid, position: GridVolumePosition) -> impl Iterator<Item = GridVolumePosition> {
    let from_structure = structure_under(obstacle_grid.cell(position.coords), position.level);
    EdgeSide::iter().filter_map(move |side| {
        let coords = position.coords.neighbor(side);
        let target_cell = obstacle_grid.get(coords)?;
        level_after_step(position.level, from_structure, side, target_cell).map(|level| coords.at(level))
    })
}

/// Returns reachable diagonal neighbors in [`EdgeSide::DIAGONALS`] order.
///
/// Diagonal moves stay on the same level and cannot enter or leave ramps. Both cells
/// beside the diagonal must be empty, with floor support and a compatible wall style.
/// The starting coordinates must be in bounds.
pub fn walkable_diagonals(obstacle_grid: &ObstacleGrid, position: GridVolumePosition) -> impl Iterator<Item = GridVolumePosition> {
    let level = position.level;
    let from_structure = structure_under(obstacle_grid.cell(position.coords), level);
    let is_on_ramp = from_structure.is_ramp();
    let from_style = from_structure.style();
    let diagonals: &[_] = if is_on_ramp { &[] } else { &EdgeSide::DIAGONALS };
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
/// `from_structure` is the supporting structure below `from_level`.
fn level_after_step(from_level: VolumeLevel, from_structure: GridStructure, side: EdgeSide, target_cell: &ObstacleCell) -> Option<VolumeLevel> {
    let mut landing = from_level;

    // A ramp exit descends at the low edge and stays level at the high edge.
    if let Some(high_side) = from_structure.high_side() {
        if side == high_side.opposite() {
            landing = landing.below()?;
        } else if side != high_side {
            return None;
        }
    }

    // Enter a ramp in the landing volume through its low edge, then use the volume above it.
    if let Some(high_side) = target_cell.at(landing).high_side() {
        if side != high_side {
            return None;
        }
        landing = landing.above()?;
    // A ramp below the landing volume can only be entered through its high edge.
    } else if let Some(high_side) = structure_under(target_cell, landing).high_side() && side != high_side.opposite() {
        return None;
    }

    can_stand_at(target_cell, landing, from_structure.style()).then_some(landing)
}

/// Checks for a walkable surface with a compatible style, excluding ramps.
fn is_flat_surface(cell: &ObstacleCell, level: VolumeLevel, from_style: Option<WallStyleKey>) -> bool {
    !structure_under(cell, level).is_ramp() && can_stand_at(cell, level, from_style)
}

/// Checks for floor support, an unblocked volume, and a compatible surface style.
fn can_stand_at(cell: &ObstacleCell, level: VolumeLevel, from_style: Option<WallStyleKey>) -> bool {
    !blocks_movement(*cell.at(level)) && cell.has_floor(level.floor_below()) && styles_join(from_style, structure_under(cell, level).style())
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
