import bevy_sprite_render::mesh2d::vertex_output::VertexOutput;
import bevy_sprite_render::mesh2d::view_bindings::globals;
import package::shaders::towers::flamethrower::common::BREAKUP;

// Light a Flamethrower's stream casts on the ground: a warm band along the burning fuel, widening as
// the fire grows, and a flickering pool where the stream hits. Added on top of whatever is
// underneath.
//
// Distances are in world units, measured from the nozzle along the stream (`s`) and across it (`u`).

// Field order and types mirror `FlameGlowUniform` in buildings_internal/src/towers/blaster/tower_flamethrower_flame.rs.
struct FlameGlow {
    intensity: f32,   // 1 while firing, lower once the stream is cut off
    near: f32,        // distance at the mesh's near edge; negative behind the nozzle
    far: f32,         // distance at the mesh's far edge
    half_width: f32,  // mesh half-width across the stream
    tail: f32,        // distance the last fuel has travelled; 0 while firing
    end: f32,         // where the stream stops
    hit: f32,         // 1 when the stream stops at a wisp
};

@group(2) @binding(0)
var<uniform> glow: FlameGlow;

const COLOR: vec3<f32> = vec3<f32>(1.0, 0.42, 0.12); // warm orange
const STREAM_LIGHT: f32 = 0.08;
const IMPACT_LIGHT: f32 = 0.15;
const IMPACT_RADIUS: f32 = 34.0;

@fragment
fn fragment(mesh: VertexOutput) -> @location(0) vec4<f32> {
    let s = mix(glow.near, glow.far, mesh.uv.x);
    let u = (0.5 - mesh.uv.y) * 2.0 * glow.half_width;
    let time = globals.time;

    // Band along the fuel: grows with the fire until it has fully caught, then cools a little toward
    // the end.
    let radius = 12.0 + 0.12 * max(s, 0.0);
    let heat = smoothstep(0.0, BREAKUP, s) * (1.0 - 0.4 * smoothstep(BREAKUP, glow.end, s));
    let span = smoothstep(glow.tail - 10.0, glow.tail + 20.0, s) * smoothstep(glow.end + 10.0, glow.end - 10.0, s);
    var light = STREAM_LIGHT * heat * span * exp(-u * u / (radius * radius));

    // Pool around the hit.
    let flicker = 0.85 + 0.15 * sin(time * 31.0) * sin(time * 17.0);
    let to_end = vec2<f32>(s - glow.end, u);
    light += glow.hit * IMPACT_LIGHT * flicker * exp(-dot(to_end, to_end) / (IMPACT_RADIUS * IMPACT_RADIUS));

    // The mesh is sized for the light to have faded out before its edges; this only guards them.
    let edge = (1.0 - smoothstep(glow.half_width * 0.8, glow.half_width, abs(u)))
        * smoothstep(glow.near, glow.near + 10.0, s) * (1.0 - smoothstep(glow.far - 10.0, glow.far, s));
    return vec4<f32>(COLOR * light * glow.intensity * edge, 1.0);
}
