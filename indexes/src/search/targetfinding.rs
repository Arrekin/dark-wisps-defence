use bevy::prelude::*;

use game_core::prelude::{AreaRange, GridCoords, TargetLevels};

use crate::{prelude::*, wisps::WispsGrid};

use super::scratch::{SEARCH_SCRATCH, SearchScratch, State};

/// Finds the nearest wisp by cardinal path distance from `range.area`.
///
/// Selects only wisps on the levels `levels` allows.
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
    levels: TargetLevels,
    ignore_obstacles: bool,
) -> Option<(GridCoords, Entity)> {
    let min_range = range.min.ceil() as usize;
    let max_range = range.max as usize;
    SEARCH_SCRATCH.with_borrow_mut(|SearchScratch { coords_visited, step_queue, .. }| {
        coords_visited.resize_and_clear(obstacle_grid.bounds);
        step_queue.clear();
        for coords in range.area.iter() {
            step_queue.push(State { cost: 0, distance: 0, key: coords });
            coords_visited.set_visited(coords);
        }
        while let Some(State { cost, distance, key: coords }) = step_queue.pop() {
            for new_coords in obstacle_grid.bounds.cardinal_neighbors(coords) {
                if coords_visited.is_visited(new_coords)
                    || (!ignore_obstacles && !obstacle_grid.cell(new_coords).is_open_ground())
                {
                    continue;
                }

                let new_distance = distance + 1;
                if new_distance >= min_range && let Some(wisp) = wisps_grid.targetable_wisps(obstacle_grid, levels, new_coords).next() {
                    return Some((new_coords, wisp));
                }

                coords_visited.set_visited(new_coords);
                if new_distance < max_range {
                    step_queue.push(State { cost: cost + 1, distance: new_distance, key: new_coords });
                }
            }
        }
        None
    })
}
