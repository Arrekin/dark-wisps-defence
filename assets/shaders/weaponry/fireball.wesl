import bevy_sprite_render::mesh2d::vertex_output::VertexOutput;
import bevy_sprite_render::mesh2d::view_bindings::globals;
import package::shaders::common::fire_ramp::fire_ramp;
import package::shaders::common::gradient_noise::{gradient_fbm_2d, gradient_noise_2d};
import package::shaders::common::hash::{hash_coords, hash_mix, hash_unit};

// Procedural fireball with a bright core, tapered flame tail, and drifting embers.
// Local x points forward along the flight; y runs across it. Distances are in core radii.
// Backward-scrolling noise shapes the flames inside an envelope compressed ahead of the core
// and stretched behind it. Density controls fire and smoke coverage; ridged noise adds bright
// streaks. A separate noise field distorts the core's edge.

// Field order and types mirror `FireballUniform` in weaponry_internal/src/fireball.rs.
struct Fireball {
    seed: f32,        // per-fireball phase offset
    tail: f32,        // tail length multiplier, following the fireball's speed
    half_length: f32, // mesh half-size along the flight, in core radii
    half_width: f32,  // mesh half-size across the flight, in core radii
};

@group(2) @binding(0)
var<uniform> fireball: Fireball;

// ── Envelope ─────────────────────────────────────────────────────────────────
const HALO: f32 = 1.7;           // flame envelope radius around the core
const FRONT_SQUASH: f32 = 1.4;   // > 1 pulls the envelope in ahead of the core
const TAIL: f32 = 2.2;           // tail length, in halo radii
const TAIL_WIDTH: f32 = 0.5;     // tail half-width at its end

// ── Flames ───────────────────────────────────────────────────────────────────
const NOISE_SCALE: f32 = 1.6;
const FLOW: f32 = 6.0;           // backward scroll of the flames
const BIAS: f32 = 0.9;           // how strongly the envelope pulls density toward the core
const THRESHOLD: f32 = 0.5;      // density at which the fire burns
const SOFTNESS: f32 = 0.12;
const SMOKE_BAND: f32 = 0.14;    // density range below the threshold that shows as smoke
const SMOKE_ALPHA: f32 = 0.35;

// ── Heat (input to `fire_ramp`) ──────────────────────────────────────────────
const HEAT_HEAD: f32 = 0.82;     // around the core
const HEAT_TAIL: f32 = 0.45;     // at the envelope's edge
const HEAT_SMOKE: f32 = 0.22;
const CORE_HEAT: f32 = 0.92;
const MAX_HEAT: f32 = 0.97;

// ── Embers ───────────────────────────────────────────────────────────────────
const EMBER_SCALE: f32 = 2.5;    // ember grid cells per core radius
const EMBER_CHANCE: f32 = 0.35;  // share of grid cells holding an ember
const EMBER_HEAT: f32 = 0.8;
const EMBER_ALPHA: f32 = 0.9;
const EMBER_SALT: u32 = 0x5bd1e995u;

@fragment
fn fragment(mesh: VertexOutput) -> @location(0) vec4<f32> {
    let along = (mesh.uv.x - 0.5) * 2.0 * fireball.half_length;
    let across = (0.5 - mesh.uv.y) * 2.0 * fireball.half_width;
    let time = globals.time + fireball.seed;

    // Envelope: 0 at the core, 1 at its edge.
    let tail_length = max(TAIL * HALO * fireball.tail, 0.001);
    let back = max(-along, 0.0);
    let tail = clamp(back / tail_length, 0.0, 1.0);
    let half_width = mix(HALO, TAIL_WIDTH, tail);
    var envelope: f32;
    if along > 0.0 {
        envelope = length(vec2<f32>(along * FRONT_SQUASH, across)) / HALO;
    } else {
        envelope = length(vec2<f32>(back / tail_length, across / max(half_width, 0.001)));
    }

    // Flame density streams backward through the envelope, swirled by a second noise field.
    let flow_point = vec2<f32>(along * 0.6 + time * FLOW, across) * NOISE_SCALE;
    let swirl = vec2<f32>(
        gradient_fbm_2d(flow_point * 0.6 + vec2<f32>(5.0 + time * 1.3, 1.0), 2),
        gradient_fbm_2d(flow_point * 0.6 + vec2<f32>(-3.0, 9.0 - time * 1.7), 2),
    ) - 0.5;
    let density = gradient_fbm_2d(flow_point + swirl * 0.9 + vec2<f32>(31.0, 7.0), 4);
    let field = density + BIAS * (0.75 - envelope);
    let fire = smoothstep(THRESHOLD, THRESHOLD + SOFTNESS, field);
    let smoke = smoothstep(THRESHOLD - SMOKE_BAND, THRESHOLD, field) * smoothstep(0.1, 0.6, tail);
    let tail_fade = 1.0 - smoothstep(0.55, 1.0, tail);
    let veins = 1.0 - abs(gradient_fbm_2d(flow_point * 1.8 + swirl * 0.3 + vec2<f32>(-17.0, 4.0), 3) * 2.0 - 1.0);

    let fire_heat = mix(HEAT_TAIL, HEAT_HEAD, clamp(1.0 - envelope, 0.0, 1.0)) + 0.5 * max(field - THRESHOLD, 0.0) + 0.25 * (veins - 0.5);
    let burning = fire / max(max(fire, smoke), 0.001);
    var heat = mix(HEAT_SMOKE, fire_heat, burning);
    var alpha = max(fire * (1.0 - 0.5 * (1.0 - veins)), smoke * SMOKE_ALPHA) * tail_fade;

    // Molten core.
    let radius = length(vec2<f32>(along, across));
    let churn = (gradient_noise_2d(vec2<f32>(along, across) * 2.5 + vec2<f32>(time * 1.1, -time * 0.9)) - 0.5) * 0.25;
    let core = 1.0 - smoothstep(0.75, 1.0, radius + churn);
    let core_heat = mix(CORE_HEAT - 0.12, CORE_HEAT, 1.0 - smoothstep(0.0, 1.0, radius));
    heat = mix(heat, core_heat, core);
    alpha = max(alpha, core);

    // Embers shed into the tail, outside the flames.
    let ember_point = vec2<f32>(along * EMBER_SCALE + time * FLOW * 1.6, across * EMBER_SCALE);
    let ember_cell = floor(ember_point);
    let ember_hash = hash_coords(ember_cell, EMBER_SALT);
    let ember_center = vec2<f32>(hash_unit(ember_hash), hash_unit(hash_mix(ember_hash))) * 0.6 + 0.2;
    let ember_roll = hash_unit(hash_mix(hash_mix(ember_hash)));
    let ember_zone = smoothstep(0.05, 0.25, tail) * (1.0 - smoothstep(0.7, 1.1, tail))
        * (1.0 - smoothstep(0.8, 1.3, abs(across) / max(half_width, 0.001))) * (1.0 - fire);
    let ember = (1.0 - smoothstep(0.0, 0.12, length(fract(ember_point) - ember_center))) * step(ember_roll, EMBER_CHANCE) * ember_zone;

    let color = mix(fire_ramp(min(heat, MAX_HEAT)), fire_ramp(EMBER_HEAT), ember);
    return vec4<f32>(color, clamp(max(alpha, ember * EMBER_ALPHA), 0.0, 1.0));
}
