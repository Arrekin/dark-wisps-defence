import bevy_sprite_render::mesh2d::vertex_output::VertexOutput;

// Soft orange glow beneath a fireball, drawn by the paint lane with additive blending.
// Intensity controls brightness; distance from the center gives a Gaussian falloff.

// Field order and types mirror `FireballGlow` in weaponry_internal/src/fireball.rs.
struct Instance {
    intensity: f32,
    mesh_half_size: f32, // in pool radii
};

const COLOR: vec3<f32> = vec3<f32>(1.0, 0.45, 0.15); // warm orange
const FALLOFF: f32 = 2.0;

fn look(mesh: VertexOutput, glow: Instance) -> vec4<f32> {
    let pool_distance = length(mesh.uv - vec2<f32>(0.5)) * 2.0 * glow.mesh_half_size;
    let light = COLOR * glow.intensity * exp(-pool_distance * pool_distance * FALLOFF);
    return vec4<f32>(light, 1.0);
}
