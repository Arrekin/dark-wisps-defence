import bevy_sprite_render::mesh2d::vertex_output::VertexOutput;

// Soft orange glow beneath a fireball, rendered with additive blending.
// Intensity controls brightness; distance from the center gives a Gaussian falloff.

// Field order and types mirror `FireballGlowUniform` in weaponry_internal/src/fireball.rs.
struct FireballGlow {
    intensity: f32,
    mesh_half_size: f32, // in pool radii
};

@group(2) @binding(0)
var<uniform> glow: FireballGlow;

const COLOR: vec3<f32> = vec3<f32>(1.0, 0.45, 0.15); // warm orange
const FALLOFF: f32 = 2.0;

@fragment
fn fragment(mesh: VertexOutput) -> @location(0) vec4<f32> {
    let pool_distance = length(mesh.uv - vec2<f32>(0.5)) * 2.0 * glow.mesh_half_size;
    let light = COLOR * glow.intensity * exp(-pool_distance * pool_distance * FALLOFF);
    return vec4<f32>(light, 1.0);
}
