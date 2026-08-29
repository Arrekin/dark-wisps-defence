use std::collections::BinaryHeap;

use game_core::prelude::GridCoords;

use crate::{emissions::EmissionsGrid, energy_supply::EnergySupplyGrid, obstacles::GridStructureType, prelude::*};

use super::common::{State, TRACKING_GRID};

const EMPTY_FIELD_MODIFIER: f32 = 1.0;
const BUILDING_FIELD_MODIFIER: f32 = 0.1;

pub fn path_find_energy_beckon(
    obstacle_grid: &ObstacleGrid,
    emissions_grid: &EmissionsGrid,
    energy_supply_grid: &EnergySupplyGrid,
    start_coords: GridCoords,
) -> Option<Vec<GridCoords>> {
    // BFS to find closest building field
    TRACKING_GRID.with_borrow_mut(|tracking| {
        tracking.resize_and_reset(obstacle_grid.bounds);
        let mut queue = BinaryHeap::new();
        queue.push(State{ cost: f32::MIN, distance: 0, coords: start_coords });
        tracking.set_tracked(start_coords, start_coords);
        while let Some(State{ distance, coords, .. }) = queue.pop() {
            for new_coords in obstacle_grid.bounds.all_neighbors(coords) {
                if tracking.is_tracked(new_coords)
                    || obstacle_grid[new_coords].has_wall()
                {
                    continue;
                }

                // If it is a diagonal move it shall be allowed only if both adjacent fields are empty
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
                let new_cost = match obstacle_grid[new_coords].structure {
                    GridStructureType::Building(entity, building_type) => {
                        // A supplier is a valid target only if it is operational:
                        //   1. has_supplier(entity) — supplier is in the active set (not disabled by player)
                        //   2. has_power()         — the power flood reached this cell (it is powered)
                        // Together they guarantee the supplier is on and emitting.
                        // See energy_supply.rs for details.
                        if building_type.is_energy_supplier()
                            && energy_supply_grid[new_coords].has_supplier(entity)
                            && energy_supply_grid[new_coords].has_power()
                        {
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
