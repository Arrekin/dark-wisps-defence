import bevy_sprite_render::mesh2d::vertex_output::VertexOutput;
import package::shaders::common::core::{CELL_SIZE, TAU};
import package::shaders::grids::obstacles::{LEVEL_ON_GROUND, LEVEL_UNDERGROUND, STRUCTURE_EMPTY, STRUCTURE_OFF_MAP, structure_at};
import package::shaders::units::scan_pulse::scan_pulse;

// Scan spot shader: the ground spot of a drone's scanning beam. Its outline is a flat ellipse,
// unit-sized in the mesh's local space, with local +x toward the drone. The spot shows the levels
// beneath it through its own style rather than geometry. A cell's level is its top: a wall or
// building top, the ground (ore, ramps and the ground over a tunnel included), or a pit floor.
// - The scan pulse reaches higher levels first, so its rings shear where the level changes.
// - Cell edges where the level changes light up inside the spot as seams, brighter as a ring
//   passes.
// - Higher levels draw brighter and lower ones dimmer.
// - The outer ring steps toward the drone on higher levels and away from it on lower ones.

// Field order and types mirror `ScanSpotVisual` in units_internal/src/expedition_drone.rs.
struct Instance {
    seed: f32,  // scan pulse phase offset
};

const BASE_COLOR: vec3<f32> = vec3<f32>(0.3, 0.85, 1.0);
const SPOT_RING_WIDTH: f32 = 0.28;
const SPOT_RING_INTENSITY: f32 = 0.9;
const SPOT_GLOW_INTENSITY: f32 = 0.15;
const SPOT_SCAN_INTENSITY: f32 = 0.25;
const SPOT_CENTER_INTENSITY: f32 = 0.7;
const SPOT_MAX_ALPHA: f32 = 0.85;

// Local size of the quad. Mirrors `SCAN_SPOT_QUAD_SIZE` in units_internal/src/expedition_drone.rs.
const QUAD_SIZE: f32 = 3.0;

// ── Levels ───────────────────────────────────────────────────────────────────
const TOF_PHASE: f32 = 2.0;        // ring phase lead per level above ground, in radians
const LEVEL_BRIGHTEN: f32 = 0.2;   // colour and alpha gain per level above ground, loss per level below
const RIM_KINK: f32 = 0.24;        // spot radii the outer ring steps toward the drone per level

// ── Seams ────────────────────────────────────────────────────────────────────
const SEAM_INTENSITY: f32 = 0.8;  // alpha of a level-change edge inside the spot
const SEAM_WIDTH: f32 = 1.5;      // world units the seam reaches to each side of the edge
const SEAM_GLOW: f32 = 1.5;       // colour gain on the seam, above 1 for bloom
const SEAM_PULSE: f32 = 0.6;      // share of the seam that brightens only as a scan ring passes

// Level of the top of the cell at `coords`, from 0 (pit floor) to 2 (wall or building top).
// Cells off the map count as ground.
fn top_level(coords: vec2<i32>) -> f32 {
    let on_ground = structure_at(coords, LEVEL_ON_GROUND);
    if on_ground == STRUCTURE_OFF_MAP { return 1.0; }
    if on_ground != STRUCTURE_EMPTY { return 2.0; }
    if structure_at(coords, LEVEL_UNDERGROUND) != STRUCTURE_EMPTY { return 1.0; }
    return 0.0;
}

// Distance from `p` to the nearest edge or corner of `cell` that borders a different level, in
// world units, or a large value when none does.
fn seam_distance(p: vec2<f32>, cell: vec2<i32>, level: f32) -> f32 {
    let local = p - vec2<f32>(cell) * CELL_SIZE;
    var distance = 1e6;
    var cardinals = array<vec2<i32>, 4>(vec2<i32>(1, 0), vec2<i32>(-1, 0), vec2<i32>(0, 1), vec2<i32>(0, -1));
    for (var i = 0; i < 4; i++) {
        let side = cardinals[i];
        if top_level(cell + side) == level { continue; }
        let outward = vec2<f32>(side);
        let to_edge = select(dot(local, -outward), CELL_SIZE - dot(local, outward), dot(outward, vec2<f32>(1.0)) > 0.0);
        distance = min(distance, to_edge);
    }
    var diagonals = array<vec2<i32>, 4>(vec2<i32>(1, 1), vec2<i32>(-1, 1), vec2<i32>(1, -1), vec2<i32>(-1, -1));
    for (var i = 0; i < 4; i++) {
        let offset = diagonals[i];
        if top_level(cell + offset) == level { continue; }
        let corner = (vec2<f32>(offset) * 0.5 + vec2<f32>(0.5)) * CELL_SIZE;
        distance = min(distance, length(local - corner));
    }
    return distance;
}

fn outer_ring(dist: f32) -> f32 {
    let ring_inner = 1.0 - SPOT_RING_WIDTH;
    return smoothstep(1.02, ring_inner + 0.02, dist) * smoothstep(ring_inner, ring_inner + 0.02, dist) * SPOT_RING_INTENSITY;
}

fn look(mesh: VertexOutput, spot: Instance) -> vec4<f32> {
    let p = mesh.world_position.xy;
    let cell = vec2<i32>(floor(p / CELL_SIZE));
    let level = top_level(cell);
    let rise = level - 1.0;

    // Spot radii from the centre, with +x toward the drone.
    let local = vec2<f32>(mesh.uv.x - 0.5, 0.5 - mesh.uv.y) * QUAD_SIZE;
    let dist = length(local);
    let rim_dist = length(local - vec2<f32>(RIM_KINK * rise, 0.0));

    // Soft inner glow
    let inner_glow = smoothstep(0.95, 0.0, dist) * SPOT_GLOW_INTENSITY;

    // Animated scan rings expanding outward, reaching higher levels first
    let ring_phase = scan_pulse(spot.seed) * TAU + TOF_PHASE * rise;
    let rings = sin(dist * 10.0 - ring_phase) * 0.5 + 0.5;
    let ring_effect = rings * smoothstep(0.95, 0.2, dist) * SPOT_SCAN_INTENSITY;

    // Center bright spot
    let center_spot = smoothstep(0.2, 0.0, dist) * SPOT_CENTER_INTENSITY;

    let gain = max(1.0 + LEVEL_BRIGHTEN * rise, 0.0);
    var alpha = (outer_ring(rim_dist) + inner_glow + ring_effect + center_spot) * gain;
    var color = BASE_COLOR * gain;

    // Level-change seams inside the spot
    let inside = smoothstep(1.02, 0.85, min(dist, rim_dist));
    let seam = (1.0 - smoothstep(0.0, SEAM_WIDTH, seam_distance(p, cell, level))) * inside;
    let lit = seam * SEAM_INTENSITY * mix(1.0, rings, SEAM_PULSE);
    color = mix(color, BASE_COLOR * SEAM_GLOW, saturate(lit));
    alpha = max(alpha, lit);

    return vec4<f32>(color, clamp(alpha, 0.0, SPOT_MAX_ALPHA));
}
