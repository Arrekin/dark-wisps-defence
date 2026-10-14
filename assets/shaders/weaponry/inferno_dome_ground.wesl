import bevy_sprite_render::mesh2d::vertex_output::VertexOutput;
import package::shaders::common::color::display_to_linear;
import package::shaders::common::value_noise::value_fbm_3d;

// `InfernoDomeGround` shader: release flash and scorch decal under an `InfernoDome`.
// Coordinates are in world units relative to dome center. Premultiplied output: scorch darkens
// ground via `alpha`, while release flash adds light (`rgb`).

// Field order and types mirror `InfernoDomeUniform` in weaponry_internal/src/inferno_dome.rs.
struct InfernoDomeGround {
    age: f32,        // elapsed seconds since spawn
    radius: f32,     // dome radius in world units
    seed: f32,       // per-entity noise offset
    half_size: f32,  // quad half-size in world units
};

@group(2) @binding(0) var<uniform> ground: InfernoDomeGround;

// Release flash
const FLASH_LIGHT: f32 = 0.25;
const FLASH_FADE: f32 = 0.45;    // exponential decay time constant (seconds)
const FLASH_COLOR: vec3<f32> = vec3<f32>(1.0, 0.36, 0.08);

// Scorch
const SCORCH: f32 = 0.2;
const SCORCH_DURATION: f32 = 2.5;  // mirrors `GROUND_DURATION` in weaponry_internal/src/inferno_dome.rs
const SCORCH_DARKNESS: f32 = 0.7;

@fragment
fn fragment(mesh: VertexOutput) -> @location(0) vec4<f32> {
    let p = (vec2<f32>(mesh.uv.x, 1.0 - mesh.uv.y) - vec2<f32>(0.5)) * 2.0 * ground.half_size;
    let d = length(p);
    let radius = ground.radius;
    let age = ground.age;
    var alpha = 0.0;

    let inside = smoothstep(radius + 10.0, radius - 14.0, d);
    let light = FLASH_COLOR * FLASH_LIGHT * exp(-age / FLASH_FADE) * inside * (0.55 + 0.45 * d / radius);

    let scorch = SCORCH * max(0.0, 1.0 - age / SCORCH_DURATION) * min(1.0, age / 0.15);
    if scorch > 0.0 {
        let noise = value_fbm_3d(vec3<f32>(p / 14.0, ground.seed), 4);
        let mark = smoothstep(radius + 4.0, radius - 18.0, d + (noise - 0.5) * 30.0) * smoothstep(0.3, 0.7, noise + 0.15);
        alpha = scorch * SCORCH_DARKNESS * mark;
    }
    return vec4<f32>(display_to_linear(light), alpha);
}
