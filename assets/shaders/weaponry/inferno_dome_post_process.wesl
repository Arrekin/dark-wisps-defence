import bevy_core_pipeline::fullscreen_vertex_shader::FullscreenVertexOutput;
import package::shaders::common::core::TAU;
import package::shaders::common::screen::{ScreenView, screen_uv_to_world, screen_world_to_uv};
import package::shaders::common::value_noise::value_noise_3d;

// Screen-space distortion pass for `InfernoDome` entities (world units):
// - Radial lens bulge inside the dome during breakup (`0..LENS_DURATION`).
// - Noise shimmer over the dome footprint (`0..SHIMMER_DURATION`).

@group(0) @binding(0) var screen_texture: texture_2d<f32>;
@group(0) @binding(1) var screen_sampler: sampler;
@group(0) @binding(2) var<uniform> camera: CameraData;
@group(0) @binding(3) var<storage, read> domes: array<InfernoDomeEntry>;

// Field order and types mirror `InfernoDomePostProcess` in weaponry_internal/src/inferno_dome_post_process.rs.
struct CameraData {
    world_pos: vec2<f32>,
    viewport_size: vec2<f32>,
    time: f32,
    dome_count: u32,
}

// Field order and types mirror `GpuInfernoDomeEntry` in weaponry_internal/src/inferno_dome_post_process.rs.
struct InfernoDomeEntry {
    center: vec2<f32>,
    radius: f32,  // dome radius in world units
    age: f32,     // elapsed seconds since spawn
}

// Radial lens bulge
const LENS: f32 = 5.0;              // max radial displacement in world units
const LENS_DURATION: f32 = 0.9;     // mirrors `BREAKUP_TIME` in assets/shaders/weaponry/inferno_dome.wesl

// Heat shimmer
const SHIMMER: f32 = 1.6;
const SHIMMER_FADE: f32 = 1.4;      // exponential decay time constant (seconds)
const SHIMMER_DURATION: f32 = 3.5;  // mirrors `SHIMMER_DURATION` in weaponry_internal/src/inferno_dome.rs

@fragment
fn fragment(in: FullscreenVertexOutput) -> @location(0) vec4<f32> {
    let view = ScreenView(camera.world_pos, camera.viewport_size);
    let world_pos = screen_uv_to_world(in.uv, view);
    let time = camera.time;

    var offset = vec2<f32>(0.0);
    for (var i = 0u; i < camera.dome_count; i++) {
        let entry = domes[i];
        let p = world_pos - entry.center;
        let d = max(length(p), 0.001);
        if d > entry.radius + 8.0 {
            continue;
        }
        let age = entry.age;

        // Radial bulge peaking at `0.5 * radius`.
        if age < LENS_DURATION {
            let lens = LENS * (1.0 - age / LENS_DURATION);
            offset -= p / d * lens * sin(0.5 * TAU * min(d / entry.radius, 1.0));
        }

        // Exponential decay shifted so `decay(SHIMMER_DURATION) == 0`.
        if age < SHIMMER_DURATION {
            let tail = exp(-SHIMMER_DURATION / SHIMMER_FADE);
            let decay = (exp(-age / SHIMMER_FADE) - tail) / (1.0 - tail);
            let shimmer = SHIMMER * decay * min(1.0, age / 0.1);
            let rise = vec2<f32>(value_noise_3d(vec3<f32>(p / 5.0, time * 3.0)), value_noise_3d(vec3<f32>(p / 5.0 + 9.0, time * 3.0))) - vec2<f32>(0.5);
            offset += rise * 2.0 * shimmer * smoothstep(entry.radius + 8.0, entry.radius - 16.0, d);
        }
    }

    let uv = screen_world_to_uv(world_pos + offset, view);
    return textureSampleLevel(screen_texture, screen_sampler, uv, 0.0);
}
