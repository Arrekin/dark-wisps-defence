#import bevy_sprite::mesh2d_vertex_output::VertexOutput
#import bevy_sprite::mesh2d_view_bindings::globals

// Pure-procedural fire wisp: a molten orb on fire.
//
// A glowing molten core — the wisp's body — wreathed in flame licks that emerge
// from its surface all around it and flicker chaotically, giving the vibe of the
// core being on fire. Each lick is one stylized flame (`flame_mask`): a teardrop
// shape mask, swayed and pinched to a point, modulated by smooth simplex noise
// scrolling outward, dissolving into breaking wisps at its tip. The licks are the
// same flame instanced around the orb, each in its own outward-radial frame with
// its own size, angle and phase. Composited additively, so the flames stay
// translucent over the orb. While the wisp travels, the licks stretch and lean
// into its wake — a light suggestion of a fireball trail. Nothing is sampled from
// a texture.

struct UniformData {
    seed: f32,      // per-instance phase offset, decorrelates a cluster of wisps
    vigor: f32,     // measured speed / sweet-spot, 0 at rest (locomotion input)
    heading_x: f32, // unit travel direction (quad-local), x (locomotion input)
    heading_y: f32, // unit travel direction (quad-local), y (locomotion input)
};

@group(2) @binding(4)
var<uniform> uniforms: UniformData;

const WISP_EFFECT_SLOTS: u32 = 8u;
struct WispEffects {
    mask: u32,
    params: array<vec4<f32>, WISP_EFFECT_SLOTS>,
};
@group(2) @binding(5)
var<uniform> effects: WispEffects;

const BRITTLE: u32 = 1u;
const TAU: f32 = 6.28318530718;

// Mesh padding factor. Mirrors `WispFireMaterial::QUAD_SCALE` in src/wisps/materials.rs:
// the mesh is built this many times the grid footprint and we scale UV by it, so the
// flame licks have room to reach beyond the cell while the orb keeps its on-screen size.
const QUAD_SCALE: f32 = 2.5;

// ── Orb (the wisp's body) ─────────────────────────────────────────────────────
const CORE_RADIUS: f32 = 0.28;  // glowing molten orb
const PULSE_SPEED: f32 = 2.5;   // orb breathing rate (kept subtle — the core stays hot)
const CORE_CHURN: f32 = 0.12;   // slow molten shimmer on the orb's surface

// ── Flame corona: the licks ringing the orb ──────────────────────────────────
const NUM_LICKS: i32 = 22;       // flame licks around the orb
const BASE_RADIUS: f32 = 0.106;  // how far inside the orb a lick is rooted (so it emerges from it)
const LICK_LEN: f32 = 0.72;     // base lick length (varied per lick)
const LICK_SWAY: f32 = 0.20;       // how far each lick swings tangentially about its slot (radians)
const LICK_SWAY_SPEED: f32 = 1.5;  // how fast it swings — the chaos, in place of drifting
// Locomotion: while moving, back-facing licks stream longer and all licks lean into
// the wake — a light physical suggestion that the flames lag the moving core.
const LICK_LEN_BIAS: f32 = 0.45;   // how much back licks lengthen / front licks shorten with speed
const LICK_COMB: f32 = 0.9;        // how hard the sides sweep back toward the wake (radians per unit speed)

// ── Flame shape mask (one lick) ──────────────────────────────────────────────
const FLAME_SIZE: f32 = 2.2;
const FLAME_WIDTH: f32 = 1.3;
const DISPLACEMENT_STRENGTH: f32 = 0.30;
const DISPLACEMENT_FREQUENCY: f32 = 5.0;
const DISPLACEMENT_EXPONENT: f32 = 1.5;
const DISPLACEMENT_SPEED: f32 = 5.0;
const TEAR_EXPONENT: f32 = 0.7;
const BASE_SHARPNESS: f32 = 4.0;

// ── Surface noise (smooth, scrolls up — animates the mask, doesn't define it) ─
const NOISE_SCALE: f32 = 3.0;
const NOISE_SPEED: f32 = 2.7;
const NOISE_GAIN: f32 = 0.5;
const NOISE_MULT: f32 = 0.35;

// ── Top falloff: dissolves each lick's tip into breaking wisps ────────────────
const FALLOFF_MIN: f32 = 0.2;
const FALLOFF_MAX: f32 = 1.3;
const FALLOFF_EXPONENT: f32 = 0.9;

// ── Simplex noise (Ashima / Gustavson 2D), ported to WGSL ─────────────────────
fn mod289v2(x: vec2<f32>) -> vec2<f32> { return x - floor(x * (1.0 / 289.0)) * 289.0; }
fn mod289v3(x: vec3<f32>) -> vec3<f32> { return x - floor(x * (1.0 / 289.0)) * 289.0; }
fn permute3(x: vec3<f32>) -> vec3<f32> { return mod289v3(((x * 34.0) + 1.0) * x); }

fn snoise(v: vec2<f32>) -> f32 {
    let C = vec4<f32>(0.211324865405187, 0.366025403784439, -0.577350269189626, 0.024390243902439);
    var i = floor(v + dot(v, C.yy));
    let x0 = v - i + dot(i, C.xx);
    var i1 = vec2<f32>(0.0, 1.0);
    if (x0.x > x0.y) {
        i1 = vec2<f32>(1.0, 0.0);
    }
    var x12 = vec4<f32>(x0.x + C.x, x0.y + C.x, x0.x + C.z, x0.y + C.z);
    x12 = vec4<f32>(x12.x - i1.x, x12.y - i1.y, x12.z, x12.w);
    i = mod289v2(i);
    let p = permute3(permute3(i.y + vec3<f32>(0.0, i1.y, 1.0)) + i.x + vec3<f32>(0.0, i1.x, 1.0));
    var m = max(vec3<f32>(0.5) - vec3<f32>(dot(x0, x0), dot(x12.xy, x12.xy), dot(x12.zw, x12.zw)), vec3<f32>(0.0));
    m = m * m;
    m = m * m;
    let x = 2.0 * fract(p * C.www) - 1.0;
    let h = abs(x) - 0.5;
    let ox = floor(x + 0.5);
    let a0 = x - ox;
    m = m * (1.79284291400159 - 0.85373472095314 * (a0 * a0 + h * h));
    let g = vec3<f32>(
        a0.x * x0.x + h.x * x0.y,
        a0.y * x12.x + h.y * x12.y,
        a0.z * x12.z + h.z * x12.w,
    );
    return 130.0 * dot(m, g);
}

// Heat → flame colour, red-dominant: deep ember → red → orange → yellow → white.
fn fire_ramp(h: f32) -> vec3<f32> {
    let ember  = vec3<f32>(0.35, 0.01, 0.00);
    let red    = vec3<f32>(0.85, 0.07, 0.02);
    let orange = vec3<f32>(1.00, 0.28, 0.04);
    let yellow = vec3<f32>(1.00, 0.65, 0.18);
    let white  = vec3<f32>(1.00, 0.92, 0.75);
    var c = ember * smoothstep(0.02, 0.18, h);
    c = mix(c, red,    smoothstep(0.18, 0.42, h));
    c = mix(c, orange, smoothstep(0.42, 0.66, h));
    c = mix(c, yellow, smoothstep(0.66, 0.86, h));
    c = mix(c, white,  smoothstep(0.86, 1.00, h));
    return c;
}

// A single stylized flame in a local frame.
//   fx: signed lateral position (0 = centreline)
//   fy: 0 at the base, 1 at the tip
// Returns a flame intensity in [0,1]: high in the hot body, falling to 0 at the
// edges and the dissolving tip. The reusable core, instanced around the orb.
fn flame_mask(fx: f32, fy: f32, t: f32, seed: f32) -> f32 {
    // Teardrop shaping: squish, sway (stronger toward the top), pinch to a point.
    var px = fx * FLAME_SIZE * FLAME_WIDTH;
    let py = (fy - 0.5) * FLAME_SIZE;
    let disp = max(0.0, sin(t * DISPLACEMENT_SPEED + py * DISPLACEMENT_FREQUENCY + seed)
                      * DISPLACEMENT_STRENGTH * pow(max(fy - 0.1, 0.0), DISPLACEMENT_EXPONENT));
    px = px + disp;
    px = px + px / pow(max(1.0 - py, 0.05), TEAR_EXPONENT); // teardrop point at the top
    let gradient = length(vec2<f32>(px, py));
    let base = 1.0 - pow(gradient, BASE_SHARPNESS);

    // Smooth simplex noise scrolling outward — animates the surface (the flicker).
    let up0 = snoise(vec2<f32>(fx, fy) * NOISE_SCALE + vec2<f32>(seed, -t * NOISE_SPEED)) * NOISE_MULT + NOISE_GAIN;
    let up1 = 0.5 + snoise(vec2<f32>(fx, fy) * NOISE_SCALE * 1.7 + vec2<f32>(seed + 4.0, -t * NOISE_SPEED * 0.8)) * NOISE_MULT + NOISE_GAIN;
    var flame = base * up0 * up1;

    // Dissolve the tip into breaking wisps.
    let falloff = smoothstep(FALLOFF_MIN, FALLOFF_MAX, pow(max(fy, 0.0), FALLOFF_EXPONENT));
    return clamp(flame - falloff, 0.0, 1.0);
}

// The corona: the strongest flame lick (max coverage) at this point. Every lick is
// the same flame, rooted just inside the orb and growing outward in its own frame,
// on an evenly-spaced slot that swings tangentially, with its own length and phase —
// so they read chaotic yet cover the orb uniformly. `wind` (locomotion) then biases
// each lick's length and growth direction toward the wake.
fn fire_corona(p: vec2<f32>, t: f32, seed: f32, wind: vec2<f32>) -> f32 {
    var best = 0.0;
    for (var k = 0; k < NUM_LICKS; k = k + 1) {
        let fk = f32(k);
        let r1 = fract(sin(fk * 12.9898 + seed * 1.7) * 43758.5453);
        let r2 = fract(sin(fk * 78.2330 + seed * 2.3) * 24634.6345);
        // Evenly spaced slot, swinging tangentially about it (the chaos at rest).
        let slot_ang = (fk + 0.5) / f32(NUM_LICKS) * TAU + sin(t * LICK_SWAY_SPEED + r1 * TAU) * LICK_SWAY;
        let slot_dir = vec2<f32>(cos(slot_ang), sin(slot_ang));
        // Length keyed to the home slot: wake-facing licks stream longer, front ones shorten.
        let len = max(LICK_LEN * (0.7 + 0.6 * r2) * (1.0 + LICK_LEN_BIAS * dot(wind, slot_dir)), 0.05);
        // Comb the growth direction toward the wake so the sides sweep back into a
        // tail instead of splaying out wide — the fireball, not a lion's mane. The 2D
        // cross product is speed·sin(angle-to-wake): continuous everywhere (it passes
        // smoothly through zero dead-front, so swaying licks never snap), strongest at
        // the sides — the licks that splay widest — and zero front and back. Pure
        // geometry, no time term, so turning re-combs smoothly.
        let comb = (slot_dir.x * wind.y - slot_dir.y * wind.x) * LICK_COMB;
        let grow_ang = slot_ang + comb;
        let dir = vec2<f32>(cos(grow_ang), sin(grow_ang));
        let perp = vec2<f32>(-dir.y, dir.x);
        let rel = p - slot_dir * BASE_RADIUS;   // base stays on the orb (uniform around it)
        let along = dot(rel, dir);              // distance outward from the root
        let lateral = dot(rel, perp);           // sideways from the lick's centreline
        let fl = flame_mask(lateral / len, along / len, t, seed + fk * 7.3);
        best = max(best, fl);
    }
    return best;
}

// ── Brittle: a cage of fine golden cracks over the core — a dense crackle (a Voronoi
// network) filling a disc the size of the core, like a 3D cage wrapping the orb. The
// wisps are too small for a few bold seams to read, so the crackle texture is the tell;
// it stays concentric to the core so it belongs to the body. Gold for now; colour is
// tuned later. Static, seeded per instance. ────────────────────────────────────────
const CRACK_DENSITY: f32 = 12.0;  // crackle cell count (higher = finer cracks)
const CRACK_W: f32 = 0.30;        // crack line half-width, in cell units
const CAGE_R: f32 = 0.18;         // cage disc radius, orb-space (covers the core)

fn vcell(p: vec2<f32>) -> vec2<f32> {
    return fract(sin(vec2<f32>(dot(p, vec2<f32>(127.1, 311.7)), dot(p, vec2<f32>(269.5, 183.3)))) * 43758.5453);
}

// Distance to the nearest Voronoi cell border (Quilez): ~0 on a crack, larger inside a cell.
fn crack_net(uv: vec2<f32>) -> f32 {
    let n = floor(uv);
    let f = fract(uv);
    var mr = vec2<f32>(0.0);
    var md = 8.0;
    for (var j = -1; j <= 1; j = j + 1) {
        for (var i = -1; i <= 1; i = i + 1) {
            let g = vec2<f32>(f32(i), f32(j));
            let r = g + vcell(n + g) - f;
            let d = dot(r, r);
            if (d < md) { md = d; mr = r; }
        }
    }
    md = 8.0;
    for (var j = -1; j <= 1; j = j + 1) {
        for (var i = -1; i <= 1; i = i + 1) {
            let g = vec2<f32>(f32(i), f32(j));
            let r = g + vcell(n + g) - f;
            let diff = r - mr;
            if (dot(diff, diff) > 1e-5) {
                md = min(md, dot(0.5 * (mr + r), normalize(diff)));
            }
        }
    }
    return md;
}

fn brittle(color: vec4<f32>, c: vec2<f32>) -> vec4<f32> {
    let gold = vec3<f32>(1.00, 0.78, 0.25);

    let r = length(c);
    let band = 1.0 - smoothstep(CAGE_R * 0.8, CAGE_R, r); // filled disc over the core
    let md = crack_net(c * CRACK_DENSITY + vec2<f32>(uniforms.seed));
    let crack = (1.0 - smoothstep(0.0, CRACK_W, md)) * band;

    let rgb = mix(color.rgb, gold, crack);
    return vec4<f32>(rgb, max(color.a, crack)); // opaque so the cage always reads
}

@fragment
fn fragment(mesh: VertexOutput) -> @location(0) vec4<f32> {
    let p = (mesh.uv - vec2<f32>(0.5)) * QUAD_SCALE; // y points down
    let r = length(p);
    let t = globals.time;

    // ── Glowing molten orb — the wisp's body ──────────────────────────────────
    // Mostly-stable and hot, with a slow molten shimmer on its surface (no hard blink).
    let pulse = 0.97 + 0.03 * sin(t * PULSE_SPEED + uniforms.seed);
    let churn = snoise(p * 5.0 + vec2<f32>(t * 0.4, -t * 0.3)) * CORE_CHURN;
    let orb_body = smoothstep(CORE_RADIUS, CORE_RADIUS * 0.1, r);
    let orb_heat = clamp(orb_body * (pulse + churn), 0.0, 1.0);

    // ── Flame licks all around it ─────────────────────────────────────────────
    // Locomotion wind: points opposite to travel, scaled by (bounded) speed; 0 at rest.
    let wind = -vec2<f32>(uniforms.heading_x, uniforms.heading_y) * min(uniforms.vigor, 1.5);
    let flame = fire_corona(p, t, uniforms.seed, wind);

    // ── Compose: additive HDR so the flames stay translucent over the orb ─────
    let orb_rgb = fire_ramp(orb_heat);
    let fire_rgb = fire_ramp(flame);
    let rgb = orb_rgb * orb_heat + fire_rgb * (0.5 + flame);

    let energy = clamp(orb_heat + flame, 0.0, 1.0);
    let edge = 1.0 - smoothstep(1.0, 1.2, r); // trim before the (padded) quad edge
    let alpha = clamp(energy * edge, 0.0, 1.0);

    var color = vec4<f32>(rgb, alpha);
    if ((effects.mask & BRITTLE) != 0u) {
        color = brittle(color, p);
    }
    return color;
}
