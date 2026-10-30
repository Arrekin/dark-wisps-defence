use std::collections::BinaryHeap;

use bevy::prelude::*;

use game_core::prelude::{AreaRange, GridCoords};

use crate::{prelude::*, wisps::WispsGrid};

use super::common::{State, VISITED_GRID};

/// Finds the nearest wisp by cardinal path distance from `range.area`.
///
/// Expects a cell range and uses its limits as path distances. The search traverses cells below
/// the minimum distance but cannot select targets there. Cells in `range.area` are starting
/// points and are not checked for targets.
/// When `ignore_obstacles` is `false`, occupied obstacle cells block traversal.
///
/// Returns the target's cell and entity, or `None` if the search finds no eligible wisp.
pub fn target_find_closest_wisp(
    obstacle_grid: &ObstacleGrid,
    wisps_grid: &WispsGrid,
    range: &AreaRange,
    ignore_obstacles: bool,
) -> Option<(GridCoords, Entity)> {
    let min_range = range.min.ceil() as usize;
    let max_range = range.max as usize;
    VISITED_GRID.with_borrow_mut(|visited_grid| {
        visited_grid.resize_and_clear(obstacle_grid.bounds);
        let mut queue = BinaryHeap::new();
        for coords in range.area.iter() {
            queue.push(State { cost: 0, distance: 0, coords });
            visited_grid.set_visited(coords);
        }
        while let Some(State{ cost, distance, coords }) = queue.pop() {
            for new_coords in obstacle_grid.bounds.cardinal_neighbors(coords) {
                if visited_grid.is_visited(new_coords)
                    || (!ignore_obstacles && !obstacle_grid[new_coords].is_empty())
                {
                    continue;
                }

                let new_distance = distance + 1;
                if new_distance >= min_range && !wisps_grid[new_coords].is_empty() {
                    return Some((new_coords, wisps_grid[new_coords][0]));
                }

                visited_grid.set_visited(new_coords);
                if new_distance < max_range {
                    queue.push(State { cost: cost + 1, distance: new_distance, coords: new_coords });
                }
            }
        }
        None
    })
}
