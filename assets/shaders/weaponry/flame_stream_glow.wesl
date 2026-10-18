import bevy_sprite_render::mesh2d::vertex_output::VertexOutput;
import bevy_sprite_render::mesh2d::view_bindings::globals;
import package::shaders::weaponry::flame_stream_common::BREAKUP;

// Additive ground light under a `FlameStream`: axial band along `[tail, end]` plus a radial pool
// at `end` when hitting a wisp (`hit == 1.0`).
// Coordinates (`s`, `u`) are in world units relative to the nozzle.

// Field order and types mirror `FlameGlow` in weaponry_internal/src/flame_stream_fire.rs.
struct Instance {
    intensity: f32,   // 1.0 while firing, `GLOW_VENTING` while venting
    near: f32,        // axial coordinate at the quad's near edge (`-GLOW_MESH_OVERHANG`)
    far: f32,         // axial coordinate at the quad's far edge (`length + GLOW_MESH_OVERHANG`)
    half_width: f32,  // quad half-width across the stream
    tail: f32,        // trailing fuel distance from nozzle
    end: f32,         // active stream end distance
    hit: f32,         // 1.0 when hitting a wisp, else 0.0
};

const COLOR: vec3<f32> = vec3<f32>(1.0, 0.42, 0.12);
const STREAM_LIGHT: f32 = 0.08;
const IMPACT_LIGHT: f32 = 0.15;
const IMPACT_RADIUS: f32 = 34.0;

fn look(mesh: VertexOutput, glow: Instance) -> vec4<f32> {
    let s = mix(glow.near, glow.far, mesh.uv.x);
    let u = (0.5 - mesh.uv.y) * 2.0 * glow.half_width;
    let time = globals.time;

    // Axial glow band along active fuel.
    let radius = 12.0 + 0.12 * max(s, 0.0);
    let heat = smoothstep(0.0, BREAKUP, s) * (1.0 - 0.4 * smoothstep(BREAKUP, glow.end, s));
    let span = smoothstep(glow.tail - 10.0, glow.tail + 20.0, s) * smoothstep(glow.end + 10.0, glow.end - 10.0, s);
    var light = STREAM_LIGHT * heat * span * exp(-u * u / (radius * radius));

    // Radial pool at the hit point.
    let flicker = 0.85 + 0.15 * sin(time * 31.0) * sin(time * 17.0);
    let to_end = vec2<f32>(s - glow.end, u);
    light += glow.hit * IMPACT_LIGHT * flicker * exp(-dot(to_end, to_end) / (IMPACT_RADIUS * IMPACT_RADIUS));

    // Soft falloff at quad boundaries.
    let edge = (1.0 - smoothstep(glow.half_width * 0.8, glow.half_width, abs(u)))
        * smoothstep(glow.near, glow.near + 10.0, s) * (1.0 - smoothstep(glow.far - 10.0, glow.far, s));
    return vec4<f32>(COLOR * light * glow.intensity * edge, 1.0);
}
