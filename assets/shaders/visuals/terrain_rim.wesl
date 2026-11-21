// Terrain-aware rim geometry for hemispherical domes (force fields, crucible fields, inferno domes).
// Gameplay territories ignore terrain height; this module adjusts only the rendered rim and shell:
// - On elevated cells (walls and buildings), the rim steps inward by `TOP_SHIFT`, and the cell
//   border connects the stepped rim segments across elevation changes.
// - Over pits and ramps, the dome extends downward: its rim expands outward up to `PIT_REACH`
//   proportionally to terrain depth (`surface_height_at`), and `shell_point` warps radial sample
//   coordinates near the rim so the hemisphere shell stretches to meet the expanded boundary.

import package::shaders::common::core::CELL_SIZE;
import package::shaders::grids::obstacles::{LEVEL_ON_GROUND, STRUCTURE_EMPTY, STRUCTURE_OFF_MAP, structure_at};
import package::shaders::underground::surface::surface_height_at;

const TOP_SHIFT: f32 = 4.0;   // inward rim shift on elevated cells, in world px
const PIT_REACH: f32 = 4.0;   // maximum outward rim expansion at pit floor depth, in world px

// Inward rim shift in world px for `coords`: `TOP_SHIFT` on walls and buildings, 0 elsewhere.
fn top_shift_at(coords: vec2<i32>) -> f32 {
    let on_ground = structure_at(coords, LEVEL_ON_GROUND);
    if on_ground == STRUCTURE_OFF_MAP || on_ground == STRUCTURE_EMPTY { return 0.0; }
    return TOP_SHIFT;
}

// Outward rim expansion in world px at `world_position`, scaled by underground depth
// (0 on ground, `PIT_REACH` on a pit floor).
fn pit_reach_at(world_position: vec2<f32>) -> f32 {
    return PIT_REACH * (1.0 - surface_height_at(world_position));
}

// Signed distance in world px to the terrain-adjusted rim (positive inside the dome, negative outside).
// `circle_rim` is the signed distance to the unadjusted circular rim (`radius - distance_from_center`),
// and `reach` is `pit_reach_at(world_position)`.
fn terrain_rim(world_position: vec2<f32>, circle_rim: f32, reach: f32) -> f32 {
    let rim = circle_rim + reach;
    let cell = vec2<i32>(floor(world_position / CELL_SIZE));
    let shift = top_shift_at(cell);
    var distance = rim - shift;
    if distance <= 0.0 { return distance; }
    let local = world_position - vec2<f32>(cell) * CELL_SIZE;
    var sides = array<vec2<i32>, 4>(vec2<i32>(1, 0), vec2<i32>(-1, 0), vec2<i32>(0, 1), vec2<i32>(0, -1));
    for (var i = 0; i < 4; i++) {
        let side = sides[i];
        let neighbour_shift = top_shift_at(cell + side);
        if neighbour_shift <= shift { continue; }
        let outward = vec2<f32>(side);
        let to_border = select(dot(local, -outward), CELL_SIZE - dot(local, outward), dot(outward, vec2<f32>(1.0)) > 0.0);
        // Clamp to the segment of the cell border between the outer and inner rim radii.
        let beyond = max(rim - neighbour_shift, 0.0);
        distance = min(distance, length(vec2<f32>(to_border, beyond)));
    }
    return distance;
}

// Maps `world_position` radially inward near a pit-expanded rim so hemisphere shell shading at
// `radius + reach` aligns with `radius`, while leaving interior coordinates unchanged.
fn shell_point(world_position: vec2<f32>, center: vec2<f32>, radius: f32, reach: f32) -> vec2<f32> {
    if reach <= 0.0 { return world_position; }
    let offset = world_position - center;
    let from_center = length(offset);
    let along = min(from_center / (radius + reach), 1.0);
    let mapped = from_center - reach * along * along * along * along;
    return center + offset * (mapped / max(from_center, 0.001));
}
