import bevy_sprite_render::mesh2d::vertex_output::VertexOutput;
import bevy_sprite_render::mesh2d::view_bindings::globals;
import package::shaders::common::fire_ramp::fire_ramp;
import package::shaders::common::value_noise::value_noise_3d;

// Pilot flame at a Flamethrower's nozzle: a small teardrop, blue at its root and orange at its tip,
// flickering and swaying. Tells the player the tower is armed. Added on top of whatever is
// underneath.
//
// Distances are in world units. The flame grows along +x from a root 1 unit in from the mesh's left
// edge.

// Field order and types mirror `PilotFlameUniform` in buildings_internal/src/towers/blaster/tower_flamethrower_flame.rs.
struct PilotFlame {
    seed: f32,  // per-tower phase offset
    size: f32,  // mesh side length
};

@group(2) @binding(0)
var<uniform> pilot: PilotFlame;

const ROOT_COLOR: vec3<f32> = vec3<f32>(0.2, 0.45, 1.0); // gas blue
const GLOW_COLOR: vec3<f32> = vec3<f32>(1.0, 0.5, 0.2); // warm orange

@fragment
fn fragment(mesh: VertexOutput) -> @location(0) vec4<f32> {
    let p = vec2<f32>(mesh.uv.x, 0.5 - mesh.uv.y) * pilot.size - vec2<f32>(1.0, 0.0);
    let time = globals.time + pilot.seed;

    let flicker = value_noise_3d(vec3<f32>(time * 9.0, 0.0, pilot.seed));
    let flame_length = 4.2 + 1.6 * flicker;
    let along = p.x / flame_length;
    let half_width = 1.9 * sqrt(max(along, 0.0)) * (1.0 - along) + 0.6;
    let sway = sin(time * 23.0) * 0.3 * along;
    let body = smoothstep(half_width, half_width * 0.4, abs(p.y + sway)) * step(-0.4, p.x) * step(p.x, flame_length);
    let color = mix(ROOT_COLOR, fire_ramp(0.7), smoothstep(0.15, 0.6, along));

    // A faint warm halo, faded out before the mesh's edge.
    let halo = exp(-dot(p, p) / 30.0) * 0.18 * (1.0 - smoothstep(pilot.size * 0.35, pilot.size * 0.5, length(p)));
    return vec4<f32>(color * body * 1.4 + GLOW_COLOR * halo, 1.0);
}
