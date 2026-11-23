import package::shaders::common::core::CELL_SIZE;
import package::shaders::common::map_light::{MAP_SUN_ELEVATION_TANGENT, MAP_SUN_GROUND_DIRECTION};
import package::shaders::common::map_info::grid_contains;
import package::shaders::grids::obstacles::{LEVEL_ON_GROUND, STRUCTURE_BUILDING, cell_structure, cell_top_level, obstacle_cell_at};

// Projects circular drop shadows onto cell top surfaces: pit floor (level 0), ground (level 1),
// or wall/building top (level 2).
//
// Each pixel evaluates the shadow at its own surface elevation relative to a reference `base`
// cell's top, shifting the shadow across elevation changes and converging with the caster at
// `height = 0` on the base surface.
//
// On the leeward (away-from-sun) side of an elevation drop, the surface ramps down at the sun's
// elevation angle (`1 / level_run` levels per world unit) so the shadow connects continuously
// across the step instead of splitting.

// ── Shadow ───────────────────────────────────────────────────────────────────
const SHADOW_ALPHA: f32 = 0.6;
const SHADOW_SPREAD_HEIGHT: f32 = 128.0;  // height (world units) at which spread and fade saturate
// Mirrored by `SHADOW_SPREAD` in visuals/src/drop_shadow.rs.
const SHADOW_SPREAD: f32 = 0.35;          // fractional diameter increase at `SHADOW_SPREAD_HEIGHT`
const SHADOW_FADE: f32 = 0.35;            // fractional opacity reduction at `SHADOW_SPREAD_HEIGHT`

// ── Levels ───────────────────────────────────────────────────────────────────
const LEVEL_SHARPEN: f32 = 0.6;  // spread scaling per level difference from base (tighter higher up, wider lower down)
// Mirrored by `LEVEL_STEP` in visuals/src/drop_shadow.rs.
const LEVEL_STEP: f32 = 24.0;    // vertical height per elevation level, in world units

// Top elevation level of `coords`: 0 (pit floor), 1 (ground, or out of bounds), or 2 (wall or building).
fn top_level(coords: vec2<i32>) -> f32 {
    if !grid_contains(coords) { return 1.0; }
    let cell = obstacle_cell_at(coords);
    if cell_structure(cell, LEVEL_ON_GROUND) == STRUCTURE_BUILDING { return 2.0; }
    return f32(cell_top_level(cell));
}

// Ray distance along `direction` from `p` to the next cell boundary on one axis (or `1e9` if parallel).
fn to_boundary(p: f32, direction: f32) -> f32 {
    if abs(direction) < 1e-5 { return 1e9; }
    let cell = floor(p / CELL_SIZE);
    let boundary = select(cell * CELL_SIZE, (cell + 1.0) * CELL_SIZE, direction > 0.0);
    return (boundary - p) / direction;
}

// Effective surface level at world position `p`, including leeward ramps down from taller cells
// within two level-steps (`2 * level_run`) toward the sun.
fn surface_height(p: vec2<f32>, level_run: f32) -> f32 {
    let direction = MAP_SUN_GROUND_DIRECTION;
    let cell = vec2<i32>(floor(p / CELL_SIZE));
    var height = top_level(cell);
    // `2 * level_run < CELL_SIZE`, so the segment toward the sun crosses at most one X and one Y boundary.
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

// Smooth radial falloff in `[0, 1]` for normalized radius `distance` (`1.0` at the disc edge).
fn disc(distance: f32) -> f32 {
    let falloff = 1.0 - clamp((distance - 0.3) / 0.7, 0.0, 1.0);
    return falloff * falloff * (3.0 - 2.0 * falloff);
}

// Surface elevation at `p` relative to the cell top at `base`, in levels.
// Precomputed once per pixel when evaluating multiple shadows sharing the same `base`.
fn surface_rise(p: vec2<f32>, base: vec2<f32>) -> f32 {
    let level_run = LEVEL_STEP / MAP_SUN_ELEVATION_TANGENT;
    return surface_height(p, level_run) - top_level(vec2<i32>(floor(base / CELL_SIZE)));
}

// Shadow alpha at `p` for a caster over `ground` at `height` world units above its base surface,
// with zero-height `diameter` and precomputed `rise = surface_rise(p, base)`.
fn cast_alpha(p: vec2<f32>, ground: vec2<f32>, rise: f32, height: f32, diameter: f32) -> f32 {
    let above = max(height - rise * LEVEL_STEP, 0.0);
    let offset = -MAP_SUN_GROUND_DIRECTION * above / MAP_SUN_ELEVATION_TANGENT;
    let spread = clamp(above / SHADOW_SPREAD_HEIGHT * (1.0 - rise * LEVEL_SHARPEN), 0.0, 1.0);
    let radius = 0.5 * diameter * (1.0 + SHADOW_SPREAD * spread);
    return SHADOW_ALPHA * (1.0 - SHADOW_FADE * spread) * disc(length(p - ground - offset) / radius);
}
