use std::collections::VecDeque;

use game_core::prelude::GridCoords;
use grids::search::{SEARCH_SCRATCH, SearchScratch, State};

use crate::{emissions::EmissionsGrid, obstacles::GridStructure, prelude::*};

const EMPTY_FIELD_MODIFIER: f32 = 1.0;
const BUILDING_FIELD_MODIFIER: f32 = 0.1;

/// Searches for an operational energy supplier, prioritizing nearby cells with stronger emissions.
///
/// On success, replaces `path` with the route, excluding the start and including the target cell.
/// Returns `false` and leaves `path` unchanged if no supplier is reachable.
pub fn path_find_energy_beckon(
    obstacle_grid: &ObstacleGrid,
    emissions_grid: &EmissionsGrid,
    start_coords: GridCoords,
    path: &mut VecDeque<GridCoords>,
) -> bool {
    SEARCH_SCRATCH.with_borrow_mut(|SearchScratch { tracking, cost_queue, .. }| {
        tracking.resize_and_clear(obstacle_grid.bounds);
        cost_queue.clear();
        cost_queue.push(State{ cost: f32::MIN, distance: 0, coords: start_coords });
        tracking.set_tracked(start_coords, start_coords);
        while let Some(State{ distance, coords, .. }) = cost_queue.pop() {
            for new_coords in obstacle_grid.bounds.all_neighbors(coords) {
                if tracking.is_tracked(new_coords)
                    || obstacle_grid[new_coords].has_wall()
                {
                    continue;
                }

                // Prevent diagonal movement past the corner of a wall or building.
                let is_diagonal_move = new_coords.x != coords.x && new_coords.y != coords.y;
                if is_diagonal_move {
                    let adjacent_x = GridCoords { x: new_coords.x, y: coords.y };
                    let adjacent_y = GridCoords { x: coords.x, y: new_coords.y };
                    if obstacle_grid[adjacent_x].has_structure() || obstacle_grid[adjacent_y].has_structure() {
                        continue;
                    }
                }

                tracking.set_tracked(new_coords, coords);
                let new_distance = distance + 1;
                let new_cost = match obstacle_grid[new_coords].elevation {
                    GridStructure::Building { slot, building_type } => {
                        if building_type.is_energy_supplier() && obstacle_grid.entry(slot).data.operational {
                            tracking.compile_path_into(start_coords, new_coords, path);
                            return true;
                        } else {
                            -emissions_grid[new_coords].energy * BUILDING_FIELD_MODIFIER + new_distance as f32
                        }
                    }
                    _ => -emissions_grid[new_coords].energy * EMPTY_FIELD_MODIFIER + new_distance as f32,
                };
                cost_queue.push(State { cost: new_cost, distance: new_distance, coords: new_coords });
            }
        }
        false
    })
}
