import bevy_sprite_render::mesh2d::vertex_output::VertexOutput;
import bevy_sprite_render::mesh2d::view_bindings::globals;
import package::shaders::common::fire_ramp::fire_ramp;
import package::shaders::common::value_noise::value_noise_3d;

// Additive nozzle pilot flame for an armed `FlameStream`.
// Local coordinates (`p`) are in world units with the flame root at `(0, 0)` (1 unit inside the
// quad's left edge), extending along `+x`.

// Field order and types mirror `PilotFlame` in weaponry_internal/src/flame_stream_fire.rs.
struct Instance {
    seed: f32,  // per-stream phase offset
    size: f32,  // quad side length in world units
};

const ROOT_COLOR: vec3<f32> = vec3<f32>(0.2, 0.45, 1.0);
const GLOW_COLOR: vec3<f32> = vec3<f32>(1.0, 0.5, 0.2);

fn look(mesh: VertexOutput, pilot: Instance) -> vec4<f32> {
    let p = vec2<f32>(mesh.uv.x, 0.5 - mesh.uv.y) * pilot.size - vec2<f32>(1.0, 0.0);
    let time = globals.time + pilot.seed;

    let flicker = value_noise_3d(vec3<f32>(time * 9.0, 0.0, pilot.seed));
    let flame_length = 4.2 + 1.6 * flicker;
    let along = p.x / flame_length;
    let half_width = 1.9 * sqrt(max(along, 0.0)) * (1.0 - along) + 0.6;
    let sway = sin(time * 23.0) * 0.3 * along;
    let body = smoothstep(half_width, half_width * 0.4, abs(p.y + sway)) * step(-0.4, p.x) * step(p.x, flame_length);
    let color = mix(ROOT_COLOR, fire_ramp(0.7), smoothstep(0.15, 0.6, along));

    let halo = exp(-dot(p, p) / 30.0) * 0.18 * (1.0 - smoothstep(pilot.size * 0.35, pilot.size * 0.5, length(p)));
    return vec4<f32>(color * body * 1.4 + GLOW_COLOR * halo, 1.0);
}
