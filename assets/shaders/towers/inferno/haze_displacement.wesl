import bevy_sprite_render::mesh2d::{vertex_output::VertexOutput, view_bindings::globals};
import package::shaders::common::core::TAU;
import package::shaders::common::displacement::world_offset_to_uv;
import package::shaders::common::value_noise::value_noise_3d;

// Heat haze around a charging `TowerInferno`. Inward-drifting radial waves between
// `front = reach * (1 - charge)` and `reach` (world units). The quad spans the haze diameter.

// Field order and types mirror `InfernoHaze` in buildings_internal/src/towers/emitter/tower_inferno_fire.rs.
struct Instance {
    reach: f32,   // outer haze radius in world units
    charge: f32,  // normalized reload progress (0..1)
};

// Haze tuning
const STRENGTH: f32 = 1.6;      // max displacement in world units
const DRIFT_SPEED: f32 = 30.0;  // inward wave speed in world units/s
const RING_SPACING: f32 = 14.0;

fn displace(in: VertexOutput, haze: Instance) -> vec4<f32> {
    let time = globals.time;
    let p = (in.uv - vec2<f32>(0.5)) * vec2<f32>(2.0, -2.0) * haze.reach;
    let d = max(length(p), 0.001);
    if d > haze.reach {
        return vec4<f32>(0.0);
    }

    // Active band spans `[front, haze.reach]`, tapering at both ends.
    let charge = haze.charge;
    let front = haze.reach * (1.0 - charge);
    let envelope = smoothstep(front - 16.0, front + 8.0, d) * smoothstep(haze.reach, haze.reach * 0.6, d) * sqrt(charge);
    if envelope <= 0.0 {
        return vec4<f32>(0.0);
    }

    let q = p / 7.0;
    let jitter = vec2<f32>(value_noise_3d(vec3<f32>(q, time * 2.0)), value_noise_3d(vec3<f32>(q + 17.0, time * 2.0))) - vec2<f32>(0.5);
    let phase = (d + DRIFT_SPEED * time) / RING_SPACING + value_noise_3d(vec3<f32>(q * 0.5, time * 0.3)) * 2.0;
    let wave = sin(TAU * phase);
    let offset = (p / d * wave * 0.7 + jitter * 0.9) * STRENGTH * envelope;

    return vec4<f32>(world_offset_to_uv(offset), 0.0, 0.0);
}
