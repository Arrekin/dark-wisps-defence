// Floor connections between neighbouring cells across obstacle grid levels (`LEVEL_*`).
// Mirrors `ObstacleGrid::connects_floor` in `indexes/src/obstacles.rs`: floors connect where they
// meet their shared edge at the same elevation and both cells are open there. A ramp's surface
// belongs to the level above the ramp structure, so its foot connects to the level below.
// Custom materials enable the shared GPU group through `visuals::shared_gpu`; lane shaders include it.

import package::shaders::grids::obstacles::{
    LEVEL_NONE, LEVEL_ON_ELEVATION, LEVEL_UNDERGROUND, ObstacleCell, cell_has_floor, cell_structure_shape, cell_wall_style_index,
    obstacle_cell_at,
};
import package::shaders::grids::openings::is_connected;
import package::shaders::grids::structure_shape::{ramp_up, shape_ramp};

// Uphill direction of the ramp in layer `level` of `cell`, or zero when that layer holds no ramp.
fn ramp_up_in(cell: ObstacleCell, level: u32) -> vec2<i32> {
    let ramp = shape_ramp(cell_structure_shape(cell, level));
    if ramp == 0u { return vec2<i32>(0); }
    return ramp_up(ramp);
}

// Level at whose base the floor of `level` meets cardinal `side`: `level` for a flat floor and at
// a ramp's crest, the level below at a ramp's foot, and `LEVEL_NONE` without a floor or along a
// ramp's sloped sides. Mirrors `ObstacleCell::floor_edge_base`.
fn floor_edge_base(cell: ObstacleCell, level: u32, side: vec2<i32>) -> u32 {
    if !cell_has_floor(cell, level) { return LEVEL_NONE; }
    if level == LEVEL_UNDERGROUND { return level; }
    let up = ramp_up_in(cell, level - 1u);
    if all(up == vec2<i32>(0)) || all(side == up) { return level; }
    if all(side == -up) { return level - 1u; }
    return LEVEL_NONE;
}

// Level of `cell` whose floor meets cardinal `side` at elevation `edge_level`, or `LEVEL_NONE`.
// A ramp in layer `edge_level` connects only through its foot, to its surface one level up.
// Mirrors `ObstacleCell::floor_level_reaching`.
fn floor_level_reaching(cell: ObstacleCell, edge_level: u32, side: vec2<i32>) -> u32 {
    let up = ramp_up_in(cell, edge_level);
    if any(up != vec2<i32>(0)) {
        if all(side == -up) && edge_level < LEVEL_ON_ELEVATION { return edge_level + 1u; }
        return LEVEL_NONE;
    }
    if floor_edge_base(cell, edge_level, side) == edge_level { return edge_level; }
    return LEVEL_NONE;
}

// Wall style index of the wall holding up the floor of `level`, or 0 when no wall does.
fn floor_wall_style(cell: ObstacleCell, level: u32) -> u32 {
    if level != LEVEL_ON_ELEVATION { return 0u; }
    return cell_wall_style_index(cell);
}

// Level of the neighbour across cardinal `side` (positive y up) whose floor connects with the
// floor of `level` at `coords`, or `LEVEL_NONE`. Floors on walls of different styles do not
// connect. Mirrors `ObstacleGrid::connects_floor`.
fn connects_floor(coords: vec2<i32>, level: u32, side: vec2<i32>) -> u32 {
    let from_cell = obstacle_cell_at(coords);
    let target_cell = obstacle_cell_at(coords + side);
    let base = floor_edge_base(from_cell, level, side);
    if base == LEVEL_NONE { return LEVEL_NONE; }
    if !is_connected(from_cell, side, target_cell, base) { return LEVEL_NONE; }
    let reached = floor_level_reaching(target_cell, base, -side);
    if reached == LEVEL_NONE { return LEVEL_NONE; }
    let from_style = floor_wall_style(from_cell, level);
    let target_style = floor_wall_style(target_cell, reached);
    if from_style != 0u && target_style != 0u && from_style != target_style { return LEVEL_NONE; }
    return reached;
}
