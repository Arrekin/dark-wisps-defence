use std::collections::VecDeque;

use game_core::prelude::GridVolumePosition;

use crate::{emissions::EmissionsGrid, obstacles::GridStructure, prelude::*};

use super::{
    scratch::{SEARCH_SCRATCH, SearchScratch, State},
    stepping::{walkable_diagonals, walkable_neighbors},
};

const EMPTY_FIELD_MODIFIER: f32 = 1.0;
const BUILDING_FIELD_MODIFIER: f32 = 0.1;

/// Searches for an operational energy supplier, prioritizing nearby cells with stronger emissions.
///
/// On success, replaces `path` with the route, excluding the start and including the target cell.
/// Returns `false` and leaves `path` unchanged if no supplier is reachable.
pub fn path_find_energy_beckon(
    obstacle_grid: &ObstacleGrid,
    emissions_grid: &EmissionsGrid,
    start: GridVolumePosition,
    path: &mut VecDeque<GridVolumePosition>,
) -> bool {
    SEARCH_SCRATCH.with_borrow_mut(|SearchScratch { tracking, cost_queue, .. }| {
        tracking.resize_and_clear(obstacle_grid.bounds);
        cost_queue.clear();
        cost_queue.push(State { cost: f32::MIN, distance: 0, key: start });
        tracking.set_tracked(start, start);
        while let Some(State { distance, key: position, .. }) = cost_queue.pop() {
            for new_position in walkable_neighbors(obstacle_grid, position).chain(walkable_diagonals(obstacle_grid, position)) {
                if tracking.is_tracked(new_position) {
                    continue;
                }

                tracking.set_tracked(new_position, position);
                let new_distance = distance + 1;
                let field_modifier = match obstacle_grid[new_position] {
                    GridStructure::Building { slot, building_type } if building_type.is_energy_supplier() && obstacle_grid.entry(slot).data.operational => {
                        tracking.compile_path_into(start, new_position, path);
                        return true;
                    }
                    GridStructure::Building { .. } => BUILDING_FIELD_MODIFIER,
                    _ => EMPTY_FIELD_MODIFIER,
                };
                let new_cost = -emissions_grid[new_position].energy * field_modifier + new_distance as f32;
                cost_queue.push(State { cost: new_cost, distance: new_distance, key: new_position });
            }
        }
        false
    })
}
