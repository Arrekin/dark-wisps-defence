import bevy_core_pipeline::fullscreen_vertex_shader::FullscreenVertexOutput;
import package::shaders::common::core::TAU;
import package::shaders::common::screen::{ScreenView, screen_uv_to_world, screen_world_to_uv};
import package::shaders::common::value_noise::value_noise_3d;

// Screen-space heat-haze distortion around charging `TowerInferno` entities.
// Inward-drifting radial waves between `front = reach * (1 - charge)` and `reach` (world units).

@group(0) @binding(0) var screen_texture: texture_2d<f32>;
@group(0) @binding(1) var screen_sampler: sampler;
@group(0) @binding(2) var<uniform> camera: CameraData;
@group(0) @binding(3) var<storage, read> infernos: array<InfernoHazeEntry>;

// Field order and types mirror `InfernoHazePostProcess` in buildings_internal/src/towers/emitter/tower_inferno_haze.rs.
struct CameraData {
    world_pos: vec2<f32>,
    viewport_size: vec2<f32>,
    time: f32,
    inferno_count: u32,
}

// Field order and types mirror `GpuInfernoHazeEntry` in buildings_internal/src/towers/emitter/tower_inferno_haze.rs.
struct InfernoHazeEntry {
    center: vec2<f32>,
    reach: f32,   // outer haze radius in world units
    charge: f32,  // normalized reload progress (0..1)
}

// Haze tuning
const STRENGTH: f32 = 1.6;      // max displacement in world units
const DRIFT_SPEED: f32 = 30.0;  // inward wave speed in world units/s
const RING_SPACING: f32 = 14.0;

@fragment
fn fragment(in: FullscreenVertexOutput) -> @location(0) vec4<f32> {
    let view = ScreenView(camera.world_pos, camera.viewport_size);
    let world_pos = screen_uv_to_world(in.uv, view);
    let time = camera.time;

    var offset = vec2<f32>(0.0);
    for (var i = 0u; i < camera.inferno_count; i++) {
        let entry = infernos[i];
        let p = world_pos - entry.center;
        let d = max(length(p), 0.001);
        if d > entry.reach {
            continue;
        }

        // Active band spans `[front, entry.reach]`, tapering at both ends.
        let charge = entry.charge;
        let front = entry.reach * (1.0 - charge);
        let envelope = smoothstep(front - 16.0, front + 8.0, d) * smoothstep(entry.reach, entry.reach * 0.6, d) * sqrt(charge);
        if envelope <= 0.0 {
            continue;
        }

        let q = p / 7.0;
        let jitter = vec2<f32>(value_noise_3d(vec3<f32>(q, time * 2.0)), value_noise_3d(vec3<f32>(q + 17.0, time * 2.0))) - vec2<f32>(0.5);
        let phase = (d + DRIFT_SPEED * time) / RING_SPACING + value_noise_3d(vec3<f32>(q * 0.5, time * 0.3)) * 2.0;
        let wave = sin(TAU * phase);
        offset += (p / d * wave * 0.7 + jitter * 0.9) * STRENGTH * envelope;
    }

    let uv = screen_world_to_uv(world_pos + offset, view);
    return textureSampleLevel(screen_texture, screen_sampler, uv, 0.0);
}
