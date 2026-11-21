import bevy_sprite_render::mesh2d::vertex_output::VertexOutput;
import bevy_sprite_render::mesh2d::view_bindings::globals;
import package::shaders::common::core::CELL_SIZE;
import package::shaders::common::fire_ramp::fire_ramp;
import package::shaders::common::noise::noise_fbm_3d_drift;
import package::shaders::grids::obstacles::{LEVEL_NONE, LEVEL_ON_ELEVATION, LEVEL_ON_GROUND, LEVEL_UNDERGROUND, STRUCTURE_BUILDING, STRUCTURE_EMPTY, STRUCTURE_OFF_MAP, structure_at};

// Ember ring at the outer edge of `TowerInferno` range, scaling with `charge`.
// The tower spawns one ring per terrain draw level (`LEVEL_UNDERGROUND`, `LEVEL_ON_GROUND`,
// `LEVEL_ON_ELEVATION`) so each ring draws beneath wisps at its level. Each cell is lit only by the
// ring matching its exposed floor (`top_level`): wall tops at `LEVEL_ON_ELEVATION`, ground
// (including buildings, ore, ramps, and covered excavations) at `LEVEL_ON_GROUND`, and open pits at
// `LEVEL_UNDERGROUND`.
// To match a hemisphere's cross-section at different heights, the ring shifts inward on elevated
// floors and outward in pits (`LEVEL_SHIFT`).
// Coordinates are in world units relative to the tower center. Output is additive light.

// Field order and types mirror `InfernoEmberRing` in buildings_internal/src/towers/emitter/tower_inferno_fire.rs.
struct Instance {
    range: f32,      // range radius in world units
    charge: f32,     // normalized reload progress (0..1)
    half_size: f32,  // quad half-size in world units
    altitude_index: u32,  // LEVEL_* code of the altitude level whose floors this ring lights
};

// Ring parameters
const RING_LIGHT: f32 = 0.1;
const RING_WIDTH: f32 = 3.0;
const RING_HEAT: f32 = 0.6;
const RING_HEAT_CAP: f32 = 0.97;
const RING_BUILD: f32 = 2.0;  // charge exponent (1 = linear)

// Levels
const LEVEL_SHIFT: f32 = 4.0;  // radial inward shift per level above ground (outward below ground), in world units

// Exposed floor level at `coords`, or `LEVEL_NONE` outside the map. Buildings do not raise the ring.
fn top_level(coords: vec2<i32>) -> u32 {
    let on_ground = structure_at(coords, LEVEL_ON_GROUND);
    if on_ground == STRUCTURE_OFF_MAP { return LEVEL_NONE; }
    if on_ground != STRUCTURE_EMPTY && on_ground != STRUCTURE_BUILDING { return LEVEL_ON_ELEVATION; }
    if structure_at(coords, LEVEL_UNDERGROUND) != STRUCTURE_EMPTY { return LEVEL_ON_GROUND; }
    return LEVEL_UNDERGROUND;
}

fn look(mesh: VertexOutput, ember_ring: Instance) -> vec4<f32> {
    let p = (vec2<f32>(mesh.uv.x, 1.0 - mesh.uv.y) - vec2<f32>(0.5)) * 2.0 * ember_ring.half_size;
    let range = ember_ring.range;
    let rise = f32(ember_ring.altitude_index) - f32(LEVEL_ON_GROUND);
    let d = length(p) + LEVEL_SHIFT * rise;
    if abs(d - range) >= 6.0 * RING_WIDTH {
        return vec4<f32>(0.0);
    }
    if top_level(vec2<i32>(floor(mesh.world_position.xy / CELL_SIZE))) != ember_ring.altitude_index {
        return vec4<f32>(0.0);
    }

    // Sample noise via the unit direction vector to avoid an angular seam at +-pi.
    let around = p / max(length(p), 0.001) * (6.0 + (d - range) / 8.0);
    let noise = noise_fbm_3d_drift(vec3<f32>(around, 0.0), globals.time, vec3<f32>(0.0, 0.0, 0.7), 3);
    let off_ring = (d - range) / RING_WIDTH;
    let band = exp(-off_ring * off_ring);
    let heat = RING_HEAT + 0.3 * (noise - 0.5) + 0.25 * smoothstep(0.35, 0.75, noise);
    let fire = fire_ramp(min(heat, RING_HEAT_CAP)) * RING_LIGHT * pow(ember_ring.charge, RING_BUILD) * band * (0.4 + 0.8 * noise);
    return vec4<f32>(fire, 0.0);
}
