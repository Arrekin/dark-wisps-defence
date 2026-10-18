import bevy_sprite_render::mesh2d::{vertex_output::VertexOutput, view_bindings::globals};
import package::shaders::common::core::TAU;
import package::shaders::common::displacement::world_offset_to_uv;
import package::shaders::common::game_time::game_time_since;
import package::shaders::common::value_noise::value_noise_3d;

// Distortion of an `InfernoDome` (world units):
// - Radial lens bulge inside the dome during breakup (`0..LENS_DURATION`).
// - Noise shimmer over the dome footprint (`0..SHIMMER_DURATION`).

// Field order and types mirror `InfernoDomeDisplacement` in weaponry_internal/src/inferno_dome.rs.
struct Instance {
    start_time: f32, // wrapped game time of the discharge
    radius: f32,     // dome radius in world units
    half_size: f32,  // quad half-size in world units
};

// Radial lens bulge
const LENS: f32 = 5.0;              // max radial displacement in world units
const LENS_DURATION: f32 = 0.9;     // mirrors `BREAKUP_TIME` in assets/shaders/weaponry/inferno_dome.wesl

// Heat shimmer
const SHIMMER: f32 = 1.6;
const SHIMMER_FADE: f32 = 1.4;      // exponential decay time constant (seconds)
const SHIMMER_DURATION: f32 = 3.5;  // mirrors `SHIMMER_DURATION` in weaponry_internal/src/inferno_dome.rs

fn displace(in: VertexOutput, dome: Instance) -> vec4<f32> {
    let p = (in.uv - vec2<f32>(0.5)) * vec2<f32>(2.0, -2.0) * dome.half_size;
    let d = max(length(p), 0.001);
    if d > dome.half_size {
        return vec4<f32>(0.0);
    }
    let age = game_time_since(dome.start_time);
    var offset = vec2<f32>(0.0);

    // Radial bulge peaking at `0.5 * radius`.
    if age < LENS_DURATION {
        let lens = LENS * (1.0 - age / LENS_DURATION);
        offset -= p / d * lens * sin(0.5 * TAU * min(d / dome.radius, 1.0));
    }

    // Exponential decay shifted so `decay(SHIMMER_DURATION) == 0`.
    if age < SHIMMER_DURATION {
        let time = globals.time;
        let tail = exp(-SHIMMER_DURATION / SHIMMER_FADE);
        let decay = (exp(-age / SHIMMER_FADE) - tail) / (1.0 - tail);
        let shimmer = SHIMMER * decay * min(1.0, age / 0.1);
        let rise = vec2<f32>(value_noise_3d(vec3<f32>(p / 5.0, time * 3.0)), value_noise_3d(vec3<f32>(p / 5.0 + 9.0, time * 3.0))) - vec2<f32>(0.5);
        offset += rise * 2.0 * shimmer * smoothstep(dome.half_size, dome.radius - 16.0, d);
    }

    return vec4<f32>(world_offset_to_uv(offset), 0.0, 0.0);
}
