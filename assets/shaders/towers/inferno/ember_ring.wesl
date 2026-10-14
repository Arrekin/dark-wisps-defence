import bevy_sprite_render::mesh2d::vertex_output::VertexOutput;
import bevy_sprite_render::mesh2d::view_bindings::globals;
import package::shaders::common::fire_ramp::fire_ramp;
import package::shaders::common::value_noise::value_fbm_3d;

// Ground ember ring at the outer edge of `TowerInferno` range, scaling with `charge`.
// Coordinates are in world units relative to the tower center. Output is premultiplied with
// `alpha = 0` (additive).

// Field order and types mirror `InfernoEmberRingUniform` in buildings_internal/src/towers/emitter/tower_inferno_fire.rs.
struct InfernoEmberRing {
    range: f32,      // range radius in world units
    charge: f32,     // normalized reload progress (0..1)
    half_size: f32,  // quad half-size in world units
};

@group(2) @binding(0) var<uniform> ember_ring: InfernoEmberRing;

// Ring parameters
const RING: f32 = 0.1;
const RING_WIDTH: f32 = 3.0;
const RING_HEAT: f32 = 0.6;
const RING_HEAT_CAP: f32 = 0.97;
const RING_BUILD: f32 = 2.0;  // charge exponent (1 = linear)

@fragment
fn fragment(mesh: VertexOutput) -> @location(0) vec4<f32> {
    let p = (vec2<f32>(mesh.uv.x, 1.0 - mesh.uv.y) - vec2<f32>(0.5)) * 2.0 * ember_ring.half_size;
    let d = length(p);
    let range = ember_ring.range;
    if abs(d - range) >= 6.0 * RING_WIDTH {
        return vec4<f32>(0.0);
    }

    // Sample noise via the unit direction vector to avoid an angular seam at +-pi.
    let around = p / max(d, 0.001) * (6.0 + (d - range) / 8.0);
    let noise = value_fbm_3d(vec3<f32>(around, globals.time * 0.7), 3);
    let off_ring = (d - range) / RING_WIDTH;
    let band = exp(-off_ring * off_ring);
    let heat = RING_HEAT + 0.3 * (noise - 0.5) + 0.25 * smoothstep(0.35, 0.75, noise);
    let fire = fire_ramp(min(heat, RING_HEAT_CAP)) * RING * pow(ember_ring.charge, RING_BUILD) * band * (0.4 + 0.8 * noise);
    return vec4<f32>(fire, 0.0);
}
