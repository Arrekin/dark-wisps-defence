import bevy_sprite_render::mesh2d::vertex_output::VertexOutput;
import bevy_sprite_render::mesh2d::view_bindings::globals;
import package::shaders::common::fire_ramp::{FIRE_HEAT_CAP, fire_ramp};
import package::shaders::common::hash::{hash_coords, hash_mix, hash_unit};
import package::shaders::common::value_noise::value_fbm_3d;
import package::shaders::weaponry::flame_stream_common::BREAKUP;

// `Flame` stream jet shader.
//
// Coordinates are in world units along the stream (`s` from nozzle) and across it (`u`).
// Active fuel spans `[tail, end]`. Noise is advected at `DRIFT` (`m = s - DRIFT * time`).
// Premultiplied output compositing four layers:
// - `soot`: dark fringe and tip smoke (`alpha > 0`).
// - `fire`: main flame body (`alpha > 0`).
// - `rod`: dense unburnt core near the nozzle (`0..ROD`).
// - `ember`: additive hash-grid sparks (`alpha = 0`).

// Field order and types mirror `FlameUniform` in weaponry_internal/src/flame_stream_fire.rs.
struct Flame {
    seed: f32,              // per-stream phase offset
    length: f32,            // quad length along the stream
    half_width: f32,        // quad half-width across the stream
    nozzle_half_width: f32, // cone half-width at `s = 0`
    spread: f32,            // cone half-width growth per unit `s`
    head: f32,              // leading fuel distance from nozzle
    tail: f32,              // trailing fuel distance from nozzle (0 while firing)
    end: f32,               // active stream end (`hit.distance` if hit, else `head`)
    hit: f32,               // 1.0 when hitting a wisp, else 0.0
    venting: f32,           // 1.0 when venting, else 0.0
};

@group(2) @binding(0)
var<uniform> flame: Flame;

// ── Jet ──────────────────────────────────────────────────────────────────────
const DRIFT: f32 = 650.0;         // noise advection speed along `s` (world units/s)
const ROD: f32 = 90.0;            // nozzle core length
const HEAD_BULGE: f32 = 4.0;      // extra half-width at the advancing `head`
const PILE_BULGE: f32 = 6.0;      // extra half-width at a wisp hit (`end`)
const TIP_BULGE: f32 = 5.0;       // extra half-width at an unobstructed tip

// ── Fire ─────────────────────────────────────────────────────────────────────
const BILLOW: f32 = 9.0;          // axial noise scale
const THRESHOLD: f32 = 0.45;      // density cutoff for flame mask
const HEAT_PEAK: f32 = 1.08;
const COOLING: f32 = 0.15;        // heat drop from `BREAKUP` to `end`
const SOOT: f32 = 0.1;

// ── Embers ───────────────────────────────────────────────────────────────────
const EMBER_SPEED: f32 = 360.0;    // axial speed (world units/s)
const EMBER_SPACING: f32 = 12.0;   // axial cell length
const EMBER_ROWS: f32 = 3.0;       // lateral cells per stream half-width
const EMBER_CHANCE: f32 = 0.3;     // occupancy probability per cell
const EMBER_FAN: f32 = 0.006;      // lateral spread rate per unit distance past `0.5 * BREAKUP`
const EMBER_RADIUS: f32 = 1.2;
const EMBER_SALT: u32 = 0x2f6b4c1du;

const ROD_COLOR: vec3<f32> = vec3<f32>(0.30, 0.09, 0.02);
const SOOT_COLOR: vec3<f32> = vec3<f32>(0.035, 0.028, 0.026);

@fragment
fn fragment(mesh: VertexOutput) -> @location(0) vec4<f32> {
    let s = mesh.uv.x * flame.length;
    let u = (0.5 - mesh.uv.y) * 2.0 * flame.half_width;
    let time = globals.time;
    let end = flame.end;
    let tail = flame.tail;
    let hit = flame.hit;

    // Cone half-width at `s`, expanded by head, hit pile-up, or free-tip bulges.
    var w = flame.nozzle_half_width + s * flame.spread;
    let from_head = (s - flame.head + 8.0) / 15.0;
    w += HEAD_BULGE * exp(-from_head * from_head) * f32(flame.head < end - 1.0);
    w += hit * PILE_BULGE * smoothstep(end - 30.0, end, s) * (1.0 - smoothstep(end, end + 10.0, s));
    w += (1.0 - hit) * TIP_BULGE * smoothstep(end - 40.0, end, s);
    let x = u / w;
    let ax = abs(x);

    // Advected noise coordinates.
    let m = s - DRIFT * time;
    let q = vec3<f32>(m / (BILLOW * 1.6), x * 1.15, time * 0.85 + flame.seed);
    let warp_x = value_fbm_3d(q * 0.7 + vec3<f32>(3.1, 1.7, 0.0), 3) - 0.5;
    let warp_y = value_fbm_3d(q * 0.7 + vec3<f32>(-5.2, 8.3, 2.0), 3) - 0.5;
    let qw = q + vec3<f32>(warp_x * 1.3, warp_y * 0.9, 0.0);
    let n = value_fbm_3d(qw, 5);
    let veins = 1.0 - abs(value_fbm_3d(qw * 2.1 + vec3<f32>(7.0, -3.0, 0.0), 3) * 2.0 - 1.0);

    // Axial fuel masks (`[tail, end]`) with noise-perturbed boundaries.
    let edge_noise = value_fbm_3d(vec3<f32>(m / 9.0, x * 2.0, time * 2.3), 3) - 0.5;
    let alive = smoothstep(end + 5.0 + hit * 5.0, end - 13.0 + hit * 11.0, s + edge_noise * 16.0)
        * smoothstep(tail - 2.0, tail + 24.0, s + edge_noise * 22.0);
    let alive_soot = smoothstep(end + 26.0, end - 6.0, s + edge_noise * 18.0) * smoothstep(tail - 12.0, tail + 34.0, s);

    // Flame density and heat profile along `s`.
    let breakup = smoothstep(ROD * 0.5, BREAKUP, s);
    let ignite = smoothstep(2.0, 18.0, s);
    let envelope = 1.0 - ax * ax;
    let field = n + mix(0.58, 0.40, breakup) * envelope - 0.2 + 0.14 * (1.0 - breakup) * (1.0 - ax);
    let fire = smoothstep(THRESHOLD, THRESHOLD + 0.075, field) * ignite * alive;

    let along = smoothstep(0.0, BREAKUP, s) * (1.0 - COOLING * smoothstep(BREAKUP, end + 1.0, s));
    var heat = HEAT_PEAK * along * (0.55 + 0.45 * envelope) + 0.4 * (field - THRESHOLD) + 0.16 * (veins - 0.5);
    heat += hit * 0.22 * smoothstep(end - 26.0, end, s);
    heat *= mix(1.0, 0.72, flame.venting);
    heat -= 0.25 * (1.0 - smoothstep(tail, tail + 40.0, s)) * step(0.5, tail);

    // Unburnt nozzle core (`0..ROD`).
    let rod_half = 2.0 + s * 0.035;
    let rod_presence = (1.0 - smoothstep(ROD * 0.45, ROD, s + (n - 0.5) * 26.0)) * step(tail + 0.5, s) * smoothstep(-1.0, 1.5, s);
    let rod = (1.0 - smoothstep(rod_half * 0.75, rod_half, abs(u))) * rod_presence * alive;
    let rod_color = mix(ROD_COLOR, fire_ramp(FIRE_HEAT_CAP), smoothstep(0.3, 1.0, abs(u) / rod_half));

    // Fringe and tip soot.
    let soot_noise = value_fbm_3d(vec3<f32>(m / (BILLOW * 2.6), x, time * 0.45 + flame.seed + 11.0), 4);
    let fringe = smoothstep(0.35, 0.95, ax + (soot_noise - 0.5) * 0.7) * (1.0 - smoothstep(1.15, 1.6, ax));
    let tip = smoothstep(end - 22.0, end + 6.0, s) * (1.0 - smoothstep(1.0, 1.5, ax)) * (1.0 - 0.6 * hit);
    let soot_zone = smoothstep(BREAKUP * 0.6, BREAKUP * 1.5, s) * max(fringe, tip);
    let soot = smoothstep(0.40, 0.62, soot_noise) * soot_zone * SOOT * alive_soot;

    // Premultiplied composite: soot -> fire -> rod.
    let soot_alpha = soot * 0.75;
    var color = SOOT_COLOR * soot_alpha;
    var alpha = soot_alpha;
    let fire_alpha = clamp(fire * (0.72 + 0.28 * veins), 0.0, 1.0);
    color = fire_ramp(min(heat, FIRE_HEAT_CAP)) * fire_alpha * 1.25 + color * (1.0 - fire_alpha * 0.9);
    alpha = fire_alpha * 0.9 + alpha * (1.0 - fire_alpha * 0.9);
    color = rod_color * rod + color * (1.0 - rod);
    alpha = rod + alpha * (1.0 - rod);

    // Additive embers.
    let ember_along = (s - EMBER_SPEED * time) / EMBER_SPACING;
    let fanned = max(s - BREAKUP * 0.5, 0.0);
    let ember_across = x * EMBER_ROWS / (1.0 + EMBER_FAN * fanned);
    let ember_cell = floor(vec2<f32>(ember_along, ember_across));
    let ember_hash = hash_coords(ember_cell, EMBER_SALT);
    let ember_center = vec2<f32>(hash_unit(hash_mix(ember_hash)), hash_unit(hash_mix(hash_mix(ember_hash)))) * 0.6 + vec2<f32>(0.2);
    let ember_offset = (fract(vec2<f32>(ember_along, ember_across)) - ember_center) * vec2<f32>(EMBER_SPACING, w * (1.0 + EMBER_FAN * fanned) / EMBER_ROWS);
    let ember_zone = smoothstep(BREAKUP * 0.5, BREAKUP, s) * alive_soot * (1.0 - smoothstep(1.2, 1.6, ax));
    let ember = (1.0 - smoothstep(EMBER_RADIUS * 0.4, EMBER_RADIUS, length(ember_offset)))
        * step(hash_unit(ember_hash), EMBER_CHANCE) * ember_zone * (1.0 - flame.venting * 0.5);
    color += fire_ramp(mix(FIRE_HEAT_CAP, 0.5, smoothstep(BREAKUP, end + 20.0, s))) * ember * 1.4;
    return vec4<f32>(color, alpha);
}
