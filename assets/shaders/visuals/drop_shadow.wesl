import bevy_sprite_render::mesh2d::vertex_output::VertexOutput;
import package::shaders::common::core::CELL_SIZE;
import package::shaders::common::map_light::{MAP_SUN_ELEVATION_TANGENT, MAP_SUN_GROUND_DIRECTION};
import package::shaders::grids::obstacles::{LEVEL_ON_GROUND, LEVEL_UNDERGROUND, STRUCTURE_EMPTY, STRUCTURE_OFF_MAP, structure_at};

// The round shadow of an object above the map, falling on the top of each cell it covers. A cell's
// top is the highest level with a floor, as a blast's level is: a wall or building top (2), the
// ground, ore, a ramp or the ground over a tunnel (1), or a pit floor (0). Each pixel shades the
// disc as seen at its own surface height, so the shadow steps where neighbouring tops differ.
// Levels count from the top of the base cell, so the shadow meets the object when it comes down
// there.
//
// Below each edge where a top drops, on the side away from the sun, the surface is taken as a slope
// as steep as the sun is high: it falls from the higher top at the edge to the lower top one level
// step further. The shadow follows that slope, so its parts on the two tops join across the edge.

// The mesh may lie anywhere that covers the shadow; the look measures from `ground`.
// Field order and types mirror `DropShadow` in visuals/src/drop_shadow.rs.
struct Instance {
    ground: vec2<f32>,  // world position below the object
    base: vec2<f32>,    // world position whose cell top `height` is measured from
    height: f32,        // object height above the base cell's top, in world units
    diameter: f32,      // shadow diameter at zero height, in world units
    opacity: f32,       // opacity multiplier
};

// ── Shadow ───────────────────────────────────────────────────────────────────
const SHADOW_ALPHA: f32 = 0.6;
const SHADOW_SPREAD_HEIGHT: f32 = 128.0;  // height at which spread and fade reach their limits
const SHADOW_SPREAD: f32 = 0.35;          // size increase at that height, relative to the zero-height size
const SHADOW_FADE: f32 = 0.35;            // opacity loss at that height

// ── Levels ───────────────────────────────────────────────────────────────────
const LEVEL_SHARPEN: f32 = 0.6;  // spread change per level: less above the base top, more below it
const LEVEL_STEP: f32 = 24.0;    // world units between levels for the shadow's offset

// Level of the top of the cell at `coords`, from 0 (pit floor) to 2 (wall or building top).
// Cells off the map count as ground.
fn top_level(coords: vec2<i32>) -> f32 {
    let on_ground = structure_at(coords, LEVEL_ON_GROUND);
    if on_ground == STRUCTURE_OFF_MAP { return 1.0; }
    if on_ground != STRUCTURE_EMPTY { return 2.0; }
    if structure_at(coords, LEVEL_UNDERGROUND) != STRUCTURE_EMPTY { return 1.0; }
    return 0.0;
}

// Distance along `direction` from `p` to the next cell boundary on one axis, or a large value when
// the direction runs along that axis.
fn to_boundary(p: f32, direction: f32) -> f32 {
    if abs(direction) < 1e-5 { return 1e9; }
    let cell = floor(p / CELL_SIZE);
    let boundary = select(cell * CELL_SIZE, (cell + 1.0) * CELL_SIZE, direction > 0.0);
    return (boundary - p) / direction;
}

// Surface height at `p` in levels. A cell's top is flat, except where a higher top lies within two
// level steps toward the sun: there the surface follows the slope down from that top's edge.
// `level_run` is the ground distance of one level step along the sun's direction.
fn surface_height(p: vec2<f32>, level_run: f32) -> f32 {
    let direction = MAP_SUN_GROUND_DIRECTION;
    let cell = vec2<i32>(floor(p / CELL_SIZE));
    var height = top_level(cell);
    // Two level steps are shorter than a cell, so the path toward the sun crosses at most one
    // boundary on each axis within them.
    let to_x = to_boundary(p.x, direction.x);
    let to_y = to_boundary(p.y, direction.y);
    let step_x = vec2<i32>(i32(sign(direction.x)), 0);
    let step_y = vec2<i32>(0, i32(sign(direction.y)));
    let first = min(to_x, to_y);
    let first_cell = cell + select(step_y, step_x, to_x < to_y);
    if first < 2.0 * level_run {
        height = max(height, top_level(first_cell) - first / level_run);
    }
    let second = max(to_x, to_y);
    if second < 2.0 * level_run {
        height = max(height, top_level(cell + step_x + step_y) - second / level_run);
    }
    return height;
}

// Alpha of the soft disc at `distance` from its centre, in disc radii.
fn disc(distance: f32) -> f32 {
    let falloff = 1.0 - clamp((distance - 0.3) / 0.7, 0.0, 1.0);
    return falloff * falloff * (3.0 - 2.0 * falloff);
}

fn look(mesh: VertexOutput, shadow: Instance) -> vec4<f32> {
    let local = mesh.world_position.xy - shadow.ground;
    let level_run = LEVEL_STEP / MAP_SUN_ELEVATION_TANGENT;
    let surface = surface_height(mesh.world_position.xy, level_run);
    let rise = surface - top_level(vec2<i32>(floor(shadow.base / CELL_SIZE)));

    let above = max(shadow.height - rise * LEVEL_STEP, 0.0);
    let offset = -MAP_SUN_GROUND_DIRECTION * above / MAP_SUN_ELEVATION_TANGENT;
    let spread = clamp(above / SHADOW_SPREAD_HEIGHT * (1.0 - rise * LEVEL_SHARPEN), 0.0, 1.0);
    let radius = 0.5 * shadow.diameter * (1.0 + SHADOW_SPREAD * spread);
    let alpha = shadow.opacity * SHADOW_ALPHA * (1.0 - SHADOW_FADE * spread) * disc(length(local - offset) / radius);
    return vec4<f32>(0.0, 0.0, 0.0, alpha);
}
