// Shape decoding and passage SDF helpers for walls and excavations.
// Decodes `StructureShape::gpu_code` (`map_objects/src/lib.rs`) from obstacle cells or preview
// uniforms without reading any GPU bindings.

import package::shaders::common::core::CELL_SIZE;

// Bit layout matching `StructureShape::gpu_code`:
// - bits 0..=2: ramp code (0 = flat, 1 = right, 2 = left, 3 = top, 4 = bottom)
// - bits 3..=6: hollowed-side bitmask (`SIDE_*`), or 0 for ramps
const RAMP_MASK: u32 = 0x7u;
const HOLLOWED_SIDES_SHIFT: u32 = 3u;

// Cardinal side bitmask constants matching `EdgeSides::bits` in `game_core/src/grid.rs`.
const SIDE_TOP: u32 = 1u;
const SIDE_RIGHT: u32 = 2u;
const SIDE_BOTTOM: u32 = 4u;
const SIDE_LEFT: u32 = 8u;
const SIDE_ALL: u32 = 15u;

// Width of the carved passage through a hollow structure, in world pixels.
const PASSAGE_WIDTH: f32 = 21.0;
// Sentinel distance when no hollowed side is present.
const NO_PASSAGE_DISTANCE: f32 = 1e6;

// Unpacks the ramp code (0 for flat shapes, 1..=4 for ramps).
fn shape_ramp(shape: u32) -> u32 {
    return shape & RAMP_MASK;
}

// Unpacks the `SIDE_*` mask of hollowed sides (0 for ramps).
fn shape_hollowed_sides(shape: u32) -> u32 {
    return shape >> HOLLOWED_SIDES_SHIFT;
}

// Returns the `SIDE_*` mask of sides open at the shape's own level: hollowed sides for flat shapes,
// or the foot (`-ramp_up`) for ramps. Mirrors `StructureShape::open_sides`.
fn shape_open_sides(shape: u32) -> u32 {
    let ramp = shape_ramp(shape);
    if ramp != 0u { return side_bit(-ramp_up(ramp)); }
    return shape_hollowed_sides(shape);
}

// Returns the `SIDE_*` bit of cardinal direction `side`; positive y points up.
fn side_bit(side: vec2<i32>) -> u32 {
    if side.y > 0 { return SIDE_TOP; }
    if side.x > 0 { return SIDE_RIGHT; }
    if side.y < 0 { return SIDE_BOTTOM; }
    return SIDE_LEFT;
}

// Tests a mask of `SIDE_*` bits for cardinal direction `side`; positive y points up.
fn has_side(sides: u32, side: vec2<i32>) -> bool {
    return (sides & side_bit(side)) != 0u;
}

// Converts a nonzero ramp code to its uphill grid direction; positive y points up.
fn ramp_up(ramp: u32) -> vec2<i32> {
    switch ramp {
        case 1u: { return vec2<i32>(1, 0); }
        case 2u: { return vec2<i32>(-1, 0); }
        case 3u: { return vec2<i32>(0, 1); }
        default: { return vec2<i32>(0, -1); }
    }
}

// Signed distance in world pixels to a single passage arm running from the cell center across
// cardinal unit vector `outward` (negative inside the passage). `p` is in cell units (`[-0.5, 0.5]`).
fn passage_arm_distance(p: vec2<f32>, outward: vec2<f32>) -> f32 {
    let local = p * CELL_SIZE;
    let along = clamp(dot(local, outward), 0.0, CELL_SIZE);
    return length(local - outward * along) - PASSAGE_WIDTH * 0.5;
}

// Signed distance in world pixels to the union of passage arms for `hollowed_sides` (negative inside).
// `p` is in cell units (`[-0.5, 0.5]`).
fn passage_distance(hollowed_sides: u32, p: vec2<f32>) -> f32 {
    var nearest = NO_PASSAGE_DISTANCE;
    var sides = array<vec2<i32>, 4>(vec2<i32>(0, 1), vec2<i32>(1, 0), vec2<i32>(0, -1), vec2<i32>(-1, 0));
    for (var i = 0; i < 4; i++) {
        if has_side(hollowed_sides, sides[i]) {
            nearest = min(nearest, passage_arm_distance(p, vec2<f32>(sides[i])));
        }
    }
    return nearest;
}
