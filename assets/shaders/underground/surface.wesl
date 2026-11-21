// Normalized surface height (`0.0` at pit floor, `1.0` at ground level) for underground terrain:
// pit floors, ramps, and the sloped cliff faces (`FACE_WIDTH` wide in the map plane) where lower
// cells meet higher neighbors. Flat excavations retain their ground roof at height `1.0`.

import package::shaders::common::core::CELL_SIZE;
import package::shaders::grids::obstacles::{LEVEL_UNDERGROUND, STRUCTURE_EMPTY, STRUCTURE_EXCAVATION, ObstacleCell, cell_structure, cell_structure_shape, obstacle_cell_at};
import package::shaders::grids::structure_shape::shape_ramp;
import package::shaders::underground::look::{FACE_WIDTH, ramp_rise};

// Smallest height difference that forms a face.
const STEP_EPSILON: f32 = 0.02;
// Sentinel distance for searches with no candidate yet.
const FAR_DISTANCE: f32 = 1e6;

fn underground_kind(cell: ObstacleCell) -> u32 {
    return cell_structure(cell, LEVEL_UNDERGROUND);
}

fn underground_ramp(cell: ObstacleCell) -> u32 {
    return shape_ramp(cell_structure_shape(cell, LEVEL_UNDERGROUND));
}

fn is_ramp(cell: ObstacleCell) -> bool {
    return underground_kind(cell) == STRUCTURE_EXCAVATION && underground_ramp(cell) != 0u;
}

// Surface height from 0 (pit floor) to 1 (ground). `p` is in cell units from the cell centre.
fn height_in(cell: ObstacleCell, p: vec2<f32>) -> f32 {
    if underground_kind(cell) == STRUCTURE_EMPTY { return 0.0; }
    if is_ramp(cell) { return ramp_rise(underground_ramp(cell), p); }
    return 1.0;
}

// A cliff surface sampled at a point inside a pit or ramp cell.
struct Cliff {
    // Surface height from 0 (pit floor) to 1 (ground); negative values lie below the floor.
    height: f32,
    // Distance from the cliff's rim, in world pixels.
    distance: f32,
    // Height difference at the rim, from 0 to 1; zero when no cliff was found.
    step: f32,
    // Unit normal in the map plane, pointing into the pit or ramp cell.
    normal: vec2<f32>,
    // Grid offset to the higher neighbour, across an edge or corner.
    offset: vec2<i32>,
}

// Returns the higher of two cliff samples. All cliffs share one slope, so the higher sample is the
// surface where they meet.
fn higher_cliff(a: Cliff, b: Cliff) -> Cliff {
    if b.height > a.height { return b; }
    return a;
}

// Finds the highest cliff at `p`, in cell units from the cell centre.
// All cliffs use the same slope, so taking the maximum joins their surfaces at intersections.
fn highest_cliff(coords: vec2<i32>, cell: ObstacleCell, p: vec2<f32>) -> Cliff {
    var best = Cliff(-FAR_DISTANCE, FAR_DISTANCE, 0.0, vec2<f32>(0.0), vec2<i32>(0));

    var cardinals = array<vec2<i32>, 4>(vec2<i32>(1, 0), vec2<i32>(-1, 0), vec2<i32>(0, 1), vec2<i32>(0, -1));
    for (var i = 0; i < 4; i++) {
        let side = cardinals[i];
        let outward = vec2<f32>(side);
        let to_edge = 0.5 - dot(p, outward);
        let on_edge = p + outward * to_edge;
        let rim = height_in(obstacle_cell_at(coords + side), on_edge - outward);
        let step = rim - height_in(cell, on_edge);
        if step <= STEP_EPSILON { continue; }
        let distance = to_edge * CELL_SIZE;
        best = higher_cliff(best, Cliff(rim - distance / FACE_WIDTH, distance, step, -outward, side));
    }

    var diagonals = array<vec2<i32>, 4>(vec2<i32>(1, 1), vec2<i32>(-1, 1), vec2<i32>(1, -1), vec2<i32>(-1, -1));
    for (var i = 0; i < 4; i++) {
        let offset = diagonals[i];
        let corner = vec2<f32>(offset) * 0.5;
        let rim = height_in(obstacle_cell_at(coords + offset), corner - vec2<f32>(offset));
        let step = rim - height_in(cell, corner);
        if step <= STEP_EPSILON { continue; }
        let from_corner = p - corner;
        let distance = length(from_corner) * CELL_SIZE;
        best = higher_cliff(best, Cliff(rim - distance / FACE_WIDTH, distance, step, normalize(from_corner + vec2<f32>(1e-6)), offset));
    }
    return best;
}

// Rendered terrain height in `[0.0, 1.0]` at `world_position`, including cliff slopes.
fn surface_height_at(world_position: vec2<f32>) -> f32 {
    let coords = vec2<i32>(floor(world_position / CELL_SIZE));
    let cell = obstacle_cell_at(coords);
    let p = world_position / CELL_SIZE - vec2<f32>(coords) - vec2<f32>(0.5);
    let height = height_in(cell, p);
    if height >= 1.0 { return 1.0; }
    return clamp(max(height, highest_cliff(coords, cell, p).height), 0.0, 1.0);
}
