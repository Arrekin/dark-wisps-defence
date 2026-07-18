use std::collections::BinaryHeap;

use game_core::prelude::GridCoords;

use crate::{emissions::EmissionsGrid, obstacles::GridStructureType, prelude::*};

use super::common::{ALL_DIRECTIONS, State, TRACKING_GRID};

const EMPTY_FIELD_MODIFIER: f32 = 1.0;
const BUILDING_FIELD_MODIFIER: f32 = 0.1;

pub fn path_find_energy_beckon(
    obstacle_grid: &ObstacleGrid,
    emissions_grid: &EmissionsGrid,
    start_coords: GridCoords,
) -> Option<Vec<GridCoords>> {
    // BFS to find closest building field
    TRACKING_GRID.with_borrow_mut(|tracking| {
        tracking.resize_and_reset(obstacle_grid.bounds());
        let mut queue = BinaryHeap::new();
        queue.push(State{ cost: f32::MIN, distance: 0, coords: start_coords });
        tracking.set_tracked(start_coords, start_coords);
        while let Some(State{ distance, coords, .. }) = queue.pop() {
            for (delta_x, delta_y) in ALL_DIRECTIONS {
                let new_coords = coords.shifted((delta_x, delta_y));
                if !new_coords.is_in_bounds(obstacle_grid.bounds())
                    || tracking.is_tracked(new_coords)
                    || obstacle_grid[new_coords].has_wall()
                {
                    continue;
                }

                // If it is a diagonal move it shall be allowed only if both adjacent fields are empty
                if delta_x.abs() == delta_y.abs() {
                    let adjacent_x: GridCoords = (coords.x + delta_x, coords.y).into();
                    let adjacent_y: GridCoords = (coords.x, coords.y + delta_y).into();
                    if obstacle_grid[adjacent_x].has_structure() || obstacle_grid[adjacent_y].has_structure() {
                        continue;
                    }
                }

                tracking.set_tracked(new_coords, coords);
                let new_distance = distance + 1;
                let new_cost = match obstacle_grid[new_coords].structure {
                    GridStructureType::Building(_, building_type) => {
                        // TODO: Currently finds the supplier even if its turned off(Disabled by player / not powered etc.)
                        if building_type.is_energy_supplier() {
                            // Compile the path by backtracking
                            return Some(tracking.compile_path(new_coords, start_coords));
                        } else {
                            -emissions_grid[new_coords].energy * BUILDING_FIELD_MODIFIER + new_distance as f32
                        }
                    }
                    _ => -emissions_grid[new_coords].energy * EMPTY_FIELD_MODIFIER + new_distance as f32,
                };
                queue.push(State { cost: new_cost, distance: new_distance, coords: new_coords });
            }
        }
        None
    })
}
