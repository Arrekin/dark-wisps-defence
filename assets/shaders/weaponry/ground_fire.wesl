// Low flames shared by Flamethrower impacts and napalm. World-space noise keeps flames
// continuous across adjacent cells. Closed sides end in a bright rim; open sides join
// neighbouring flames.

import package::shaders::common::core::CELL_HALF;
import package::shaders::common::value_noise::value_fbm_3d;

// Bit flags for `ground_fire`'s `open` argument. Open a side when its neighbour also burns.
// Open a corner when both adjacent neighbours and the diagonal neighbour burn.
const GROUND_FIRE_ALL_CLOSED: u32 = 0u;
const GROUND_FIRE_OPEN_RIGHT: u32 = 1u;
const GROUND_FIRE_OPEN_TOP: u32 = 2u;
const GROUND_FIRE_OPEN_LEFT: u32 = 4u;
const GROUND_FIRE_OPEN_BOTTOM: u32 = 8u;
const GROUND_FIRE_OPEN_TOP_RIGHT: u32 = 16u;
const GROUND_FIRE_OPEN_TOP_LEFT: u32 = 32u;
const GROUND_FIRE_OPEN_BOTTOM_LEFT: u32 = 64u;
const GROUND_FIRE_OPEN_BOTTOM_RIGHT: u32 = 128u;

// Negative sentinel used when all sides and corners are open.
const NO_BOUNDARY: f32 = -1.0e4;

struct GroundFire {
    // Flame coverage, 0..1.
    coverage: f32,
    // Rim brightness before the caller applies its intensity multiplier.
    rim: f32,
};

// Signed distance to the cell's closed boundary: negative inside, positive outside.
// `local` is relative to the cell centre. Open sides and corners impose no boundary.
fn ground_fire_past_boundary(local: vec2<f32>, open: u32) -> f32 {
    let past_side = vec4<f32>(local.x, local.y, -local.x, -local.y) - CELL_HALF;
    var past = NO_BOUNDARY;
    if (open & GROUND_FIRE_OPEN_RIGHT) == 0u { past = max(past, past_side.x); }
    if (open & GROUND_FIRE_OPEN_TOP) == 0u { past = max(past, past_side.y); }
    if (open & GROUND_FIRE_OPEN_LEFT) == 0u { past = max(past, past_side.z); }
    if (open & GROUND_FIRE_OPEN_BOTTOM) == 0u { past = max(past, past_side.w); }
    // Closed corners keep flames out of an unlit diagonal cell even when both sides are open.
    if (open & GROUND_FIRE_OPEN_TOP_RIGHT) == 0u { past = max(past, min(past_side.x, past_side.y)); }
    if (open & GROUND_FIRE_OPEN_TOP_LEFT) == 0u { past = max(past, min(past_side.z, past_side.y)); }
    if (open & GROUND_FIRE_OPEN_BOTTOM_LEFT) == 0u { past = max(past, min(past_side.z, past_side.w)); }
    if (open & GROUND_FIRE_OPEN_BOTTOM_RIGHT) == 0u { past = max(past, min(past_side.x, past_side.w)); }
    return past;
}

// `local` is the point relative to the cell centre and `world` the same point in world space,
// both in world units. `time` drives the animation.
fn ground_fire(local: vec2<f32>, world: vec2<f32>, time: f32, open: u32) -> GroundFire {
    let noise = value_fbm_3d(vec3<f32>(world / 5.0, time * 2.4), 4);
    let past = ground_fire_past_boundary(local, open);
    let inside = 1.0 - smoothstep(-3.0, 2.0, past + (noise - 0.5) * 7.0);
    let coverage = smoothstep(0.5, 0.64, noise + 0.1) * inside;
    let rim = smoothstep(2.5, 0.0, abs(past)) * 0.25 * (0.6 + 0.4 * noise);
    return GroundFire(coverage, rim);
}
