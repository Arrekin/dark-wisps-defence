use std::collections::BinaryHeap;

use bevy::prelude::*;

use game_core::prelude::GridCoords;

use crate::{prelude::*, wisps::WispsGrid};

use super::common::{State, VISITED_GRID};

/// Finds the closest wisp
/// `ignore_obstacles` ignores all grid obstacles
/// `range` is the maximum searching range, diagonal moves are not allowed
/// Returns grid coords and entity id of the closest wisp or None if no wisp is found
pub fn target_find_closest_wisp(
    obstacle_grid: &Res<ObstacleGrid>,
    wisps_grid: &Res<WispsGrid>,
    start_coords: impl IntoIterator<Item = GridCoords>,
    range: usize,
    ignore_obstacles: bool,
) -> Option<(GridCoords, Entity)> {
    VISITED_GRID.with_borrow_mut(|visited_grid| {
        visited_grid.resize_and_reset(obstacle_grid.bounds);
        let mut queue = BinaryHeap::new();
        start_coords.into_iter().for_each(
            |coords| {
                queue.push(State{cost: usize::MIN, distance: 0, coords });
                visited_grid.set_visited(coords);
            }
        );
        while let Some(State{ cost, distance, coords }) = queue.pop() {
            for new_coords in obstacle_grid.bounds.cardinal_neighbors(coords) {
                if visited_grid.is_visited(new_coords)
                    || (!ignore_obstacles && !obstacle_grid[new_coords].is_empty())
                {
                    continue;
                }

                if !wisps_grid[new_coords].is_empty() {
                    return Some((new_coords, wisps_grid[new_coords][0]));
                }

                visited_grid.set_visited(new_coords);
                let new_distance = distance + 1;
                if new_distance < range {
                    queue.push(State { cost: cost + 1, distance: new_distance, coords: new_coords });
                }
            }
        }
        None
    })
}
