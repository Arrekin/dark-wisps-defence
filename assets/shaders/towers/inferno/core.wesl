import bevy_sprite_render::mesh2d::vertex_output::VertexOutput;
import bevy_sprite_render::mesh2d::view_bindings::globals;
import package::shaders::common::color::linear_to_display;
import package::shaders::common::core::TAU;
import package::shaders::common::fire_ramp::{FIRE_HEAT_CAP, fire_ramp};
import package::shaders::common::value_noise::value_noise_3d;

// Charging glow overlay for `TowerInferno`. Quad matches the tower sprite bounds.
// Masks the central square core and dark sprite seams from `sprite_texture`.
// Premultiplied output: core occludes the sprite (`alpha = cover`), seams add light (`alpha = 0`).

// Field order and types mirror `InfernoCoreUniform` in buildings_internal/src/towers/emitter/tower_inferno_fire.rs.
struct InfernoCore {
    charge: f32,  // normalized reload progress (0..1)
};

@group(2) @binding(0) var<uniform> core: InfernoCore;
@group(2) @binding(1) var sprite_texture: texture_2d<f32>;
@group(2) @binding(2) var sprite_sampler: sampler;

// Heat
const HEAT_FULL: f32 = 0.74;      // core heat at full charge
const THROB: f32 = 0.12;
const THROBS_PER_SECOND: f32 = 1.6;

// UV offsets from sprite center (0.5, 0.5)
const CORE_HALF: f32 = 0.16;      // half-extent of the Emitter sprite's center square
const SEAM_REACH: f32 = 0.2;      // max seam glow expansion beyond `CORE_HALF` at full charge
const METAL_WARMTH: f32 = 0.06;

fn heat_color(heat: f32) -> vec3<f32> {
    return fire_ramp(clamp(heat, 0.0, FIRE_HEAT_CAP));
}

@fragment
fn fragment(mesh: VertexOutput) -> @location(0) vec4<f32> {
    let time = globals.time;

    let throb = 1.0 + THROB * sin(time * THROBS_PER_SECOND * TAU) * smoothstep(0.7, 1.0, core.charge);
    let heat = mix(0.1, HEAT_FULL, core.charge) * throb * smoothstep(0.0, 0.08, core.charge);
    let lit = smoothstep(0.08, 0.3, heat);

    let sprite_uv = mesh.uv;
    let sprite = textureSample(sprite_texture, sprite_sampler, sprite_uv);
    let body = sprite.a;
    let from_center = max(abs(sprite_uv.x - 0.5), abs(sprite_uv.y - 0.5));
    let luminance = dot(linear_to_display(sprite.rgb), vec3<f32>(0.3, 0.59, 0.11));

    let core_mask = 1.0 - smoothstep(CORE_HALF - 0.015, CORE_HALF + 0.01, from_center);
    let reach = CORE_HALF + SEAM_REACH * smoothstep(0.1, 1.0, core.charge);
    let seam = (1.0 - smoothstep(0.06, 0.2, luminance)) * (1.0 - smoothstep(reach - 0.07, reach, from_center));
    let flicker = value_noise_3d(vec3<f32>(sprite_uv * 18.0, time * 1.5));
    let glow = heat_color(heat * (0.85 + 0.25 * flicker));

    var color = heat_color(heat * 0.8) * seam * lit * 0.8 * body;
    color += heat_color(heat * 0.6) * METAL_WARMTH * lit * (1.0 - core_mask) * body;
    // The hot core covers the sprite.
    let cover = core_mask * lit * body;
    color = glow * 1.15 * cover + color * (1.0 - cover);
    return vec4<f32>(color, cover);
}
