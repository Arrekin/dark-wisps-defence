// Cell connectivity and hollow-passage boundary queries across obstacle grid levels (`LEVEL_*`).
// Mirrors the open-side rules of `GridStructure` in `indexes/src/obstacles.rs`.
// Custom materials enable the shared GPU group through `visuals::shared_gpu`; lane shaders include it.

import package::shaders::common::core::CELL_SIZE;
import package::shaders::grids::obstacles::{LEVEL_ON_ELEVATION, STRUCTURE_BUILDING, STRUCTURE_EMPTY, ObstacleCell, cell_structure, cell_structure_shape, obstacle_cell_at, structure_shape_at};
import package::shaders::grids::structure_shape::{SIDE_ALL, has_side, shape_open_sides, shape_hollowed_sides};

// Maximum grid borders stepped by DDA raycasts (`2n` borders cover a ray spanning `n` cells).
const MAX_CROSSINGS: i32 = 8;

// `SIDE_*` bitmask of edges open at `level`: all sides for empty space and buildings, none for
// solid earth, ore, and off-map cells, and `shape_open_sides` for walls and excavations.
fn open_sides(cell: ObstacleCell, level: u32) -> u32 {
    let structure = cell_structure(cell, level);
    if structure == STRUCTURE_EMPTY || structure == STRUCTURE_BUILDING { return SIDE_ALL; }
    return shape_open_sides(cell_structure_shape(cell, level));
}

// True if `cell` is open on cardinal `side` at `level`. Mirrors `GridStructure::has_side_open`.
fn has_side_open(cell: ObstacleCell, level: u32, side: vec2<i32>) -> bool {
    return has_side(open_sides(cell, level), side);
}

// True if `cell` and its neighbor `across` (in cardinal direction `side`) are both open on their
// shared edge at `level`.
fn is_connected(cell: ObstacleCell, side: vec2<i32>, across: ObstacleCell, level: u32) -> bool {
    return has_side_open(cell, level, side) && has_side_open(across, level, -side);
}

// `SIDE_*` bitmask of hollowed sides at `coords` and `level`, or 0 if the cell is not hollow.
fn hollowed_sides_at(coords: vec2<i32>, level: u32) -> u32 {
    return shape_hollowed_sides(structure_shape_at(coords, level));
}

// True if stepping in cardinal direction `side` between two cells crosses the unhollowed wall of a
// hollow structure on either side of the border.
fn is_closed_border(from_hollowed_sides: u32, to_hollowed_sides: u32, side: vec2<i32>) -> bool {
    return (from_hollowed_sides != 0u && !has_side(from_hollowed_sides, side))
        || (to_hollowed_sides != 0u && !has_side(to_hollowed_sides, -side));
}

// Raycasts from `origin` along unit `direction` up to `max_distance` (in world pixels) using 2D DDA,
// returning the distance to the first closed hollow-structure border on `level` (or `max_distance`
// if none is hit, or when `level` is `LEVEL_NONE`).
fn closed_border_distance(origin: vec2<f32>, direction: vec2<f32>, max_distance: f32, level: u32) -> f32 {
    if level > LEVEL_ON_ELEVATION { return max_distance; }
    let start = origin / CELL_SIZE;
    var coords = vec2<i32>(floor(start));
    let step = vec2<i32>(sign(direction));
    // Ray distance (in cells) between consecutive vertical and horizontal grid lines.
    let crossing_interval = 1.0 / max(abs(direction), vec2<f32>(1e-6));
    let to_border = select(start - floor(start), floor(start) + 1.0 - start, direction > vec2<f32>(0.0));
    var next_crossing = select(vec2<f32>(1e6), to_border * crossing_interval, step != vec2<i32>(0));
    let reach = max_distance / CELL_SIZE;

    var hollowed_sides = hollowed_sides_at(coords, level);
    for (var i = 0; i < MAX_CROSSINGS; i++) {
        let crosses_x = next_crossing.x <= next_crossing.y;
        let travelled = select(next_crossing.y, next_crossing.x, crosses_x);
        if travelled >= reach { break; }
        let side = select(vec2<i32>(0, step.y), vec2<i32>(step.x, 0), crosses_x);
        let next_coords = coords + side;
        let next_hollowed_sides = hollowed_sides_at(next_coords, level);
        if is_closed_border(hollowed_sides, next_hollowed_sides, side) { return travelled * CELL_SIZE; }
        coords = next_coords;
        hollowed_sides = next_hollowed_sides;
        next_crossing += select(vec2<f32>(0.0, crossing_interval.y), vec2<f32>(crossing_interval.x, 0.0), crosses_x);
    }
    return max_distance;
}

// True if the segment from `start` to `end` (in world pixels) does not cross any closed
// hollow-structure border on `level`.
fn is_clear_of_closed_borders(start: vec2<f32>, end: vec2<f32>, level: u32) -> bool {
    let offset = end - start;
    let distance = length(offset);
    if distance < 1e-3 { return true; }
    return closed_border_distance(start, offset / distance, distance, level) >= distance;
}

// True if the hollow structure at `coords` on `level` opens onto a non-hollow cell across `side`
// (forming an exterior entrance rather than continuing a tunnel into another hollow cell).
fn is_entrance(coords: vec2<i32>, side: vec2<i32>, level: u32) -> bool {
    let cell = obstacle_cell_at(coords);
    if shape_hollowed_sides(cell_structure_shape(cell, level)) == 0u { return false; }
    let across = obstacle_cell_at(coords + side);
    return shape_hollowed_sides(cell_structure_shape(across, level)) == 0u && is_connected(cell, side, across, level);
}
