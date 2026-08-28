#import bevy_sprite::mesh2d_vertex_output::VertexOutput
#import bevy_sprite::mesh2d_view_bindings::globals
#import dwd::core::{PI, TAU}
#import dwd::value_noise::{dwd_value_hash_2d, dwd_value_noise_2d}
#import dwd::voronoi_border::dwd_voronoi_border_2d

// Pure-procedural electric wisp: a contained plasma globe.
//
// A white-hot nucleus throws jagged violet arcs out toward its silhouette,
// striking in flickering bursts — fire, fade, restrike at fresh angles — while
// sparks twinkle off the rim. Faster travel makes it crackle harder and sheds
// its sparks into the wake. Nothing is sampled from a texture.

struct UniformData {
    seed: f32,      // per-instance phase offset, decorrelates a cluster of wisps
    vigor: f32,     // measured speed / sweet-spot; scales the crackle (0 at rest)
    heading_x: f32, // unit travel direction (quad-local), x
    heading_y: f32, // unit travel direction (quad-local), y
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

// ── Striking & arcs ──────────────────────────────────────────────────────────
const STRIKE_RATE: f32 = 8.0; // strikes per second per arc
const STRIKE_DECAY: f32 = 4.0; // how fast a strike fades within its interval
const NUM_ARCS: i32 = 8;
const ARC_WANDER: f32 = 0.6;  // how far an arc's centreline swings, radians
const ARC_CORE: f32 = 0.05;   // hot core half-width (arc-length units)
const ARC_GLOW: f32 = 0.42;   // soft halo half-width

// ── Hashes / value noise ─────────────────────────────────────────────────────
fn hash11(x: f32) -> f32 {
    return fract(sin(x * 127.1) * 43758.5453123);
}
// Smallest signed angle a - b, wrapped to (-PI, PI].
fn angle_diff(a: f32, b: f32) -> f32 {
    let d = a - b;
    return d - TAU * floor((d + PI) / TAU);
}

// One jagged arc from the core toward the rim. `base` is its striking angle;
// the centreline wanders in angle as it climbs outward, giving the kinked path.
fn arc(r: f32, ang: f32, base: f32, seed: f32, t: f32) -> f32 {
    let wander = (dwd_value_noise_2d(vec2<f32>(r * 5.0 + seed, t)) - 0.5) * (ARC_WANDER * 2.0)
               + (dwd_value_noise_2d(vec2<f32>(r * 13.0 + seed * 2.0, t * 1.7)) - 0.5) * ARC_WANDER;
    let centre = base + wander;
    let d = abs(angle_diff(ang, centre)) * r; // angular gap → screen-space thinness
    let core = 1.0 - smoothstep(0.0, ARC_CORE, d);
    let glow = 1.0 - smoothstep(0.0, ARC_GLOW, d);
    return core + glow * 0.35;
}

// ── Brittle: a cage of fine golden cracks over the core — a dense crackle (a Voronoi
// network) filling a disc, like a 3D cage wrapping the globe. A few bold seams are
// invisible at this size, so the crackle texture is the tell, and a disc of cracks
// reads nothing like the radial arcs. Gold for now; colour tuned later. Static, seeded.
const CRACK_DENSITY: f32 = 12.0;  // crackle cell count (higher = finer cracks)
const CRACK_W: f32 = 0.30;        // crack line half-width, in cell units
const CAGE_R: f32 = 0.34;         // cage disc radius (r = length(p)*2 space, covers the core)

fn brittle(color: vec4<f32>, p: vec2<f32>) -> vec4<f32> {
    let gold = vec3<f32>(1.00, 0.78, 0.25);

    let r = length(p) * 2.0;
    let band = 1.0 - smoothstep(CAGE_R * 0.8, CAGE_R, r); // filled disc over the core
    let md = dwd_voronoi_border_2d(p * CRACK_DENSITY + vec2<f32>(uniforms.seed));
    let crack = (1.0 - smoothstep(0.0, CRACK_W, md)) * band;

    let rgb = mix(color.rgb, gold, crack);
    return vec4<f32>(rgb, max(color.a, crack)); // opaque so the cage always reads
}

@fragment
fn fragment(mesh: VertexOutput) -> @location(0) vec4<f32> {
    let hot = vec3<f32>(1.00, 0.95, 1.00);  // white-hot cores
    let volt = vec3<f32>(0.55, 0.25, 1.00); // violet high-voltage glow

    let p = mesh.uv - vec2<f32>(0.5);
    let r = length(p) * 2.0; // 0 at centre, ~1 at the inscribed edge
    let ang = atan2(p.y, p.x);
    let crackle = 0.5 + 1.5 * uniforms.vigor; // agitation: harder when travelling

    // Arcs reach to the rim, each striking on its own staggered schedule so the
    // globe crackles continuously rather than strobing all at once.
    var bolts = 0.0;
    for (var k = 0; k < NUM_ARCS; k = k + 1) {
        let fk = f32(k);
        let phase = globals.time * STRIKE_RATE + uniforms.seed + fk * 0.37;
        let strike = floor(phase);
        let env = exp(-fract(phase) * STRIKE_DECAY);            // fire then fade
        let base = hash11(strike + fk * 17.0 + uniforms.seed) * TAU; // re-aim each strike
        let live = step(0.35, hash11(strike * 1.7 + fk * 5.0 + uniforms.seed)); // some skip
        bolts += arc(r, ang, base, fk * 9.0 + uniforms.seed, globals.time * 1.3) * env * live;
    }
    bolts *= crackle;

    // White-hot nucleus, surging with a shared strike pulse.
    let surge = exp(-fract(globals.time * STRIKE_RATE + uniforms.seed) * STRIKE_DECAY);
    let core_intensity = smoothstep(0.22, 0.0, r);
    let core = core_intensity * (0.75 + 0.25 * sin(globals.time * 25.0 + uniforms.seed) + surge * 0.6);

    // Sparks: sparse twinkling points in the outer body; while travelling, extra
    // sparks light up behind the heading, shedding into the wake.
    let heading = vec2<f32>(uniforms.heading_x, uniforms.heading_y);
    let dir = p / max(length(p), 1e-4);
    let back = max(0.0, -dot(dir, heading)); // 1 directly behind travel, 0 ahead
    let sp = p * 9.0;
    let twinkle = step(0.92, fract(dwd_value_hash_2d(floor(sp)) + globals.time * 3.0));
    let spark_shape = (1.0 - smoothstep(0.0, 0.25, length(fract(sp) - 0.5))) * smoothstep(0.3, 0.95, r);
    let spark = twinkle * spark_shape * (0.5 + 1.2 * back * uniforms.vigor);

    // Compose: hot where bright, violet in the falloff, plus a breathing corona.
    var energy = core + bolts + spark * 0.6;
    var rgb = mix(volt, hot, clamp(energy, 0.0, 1.0)) * energy;
    let corona = exp(-r * 3.0) * (0.15 + 0.1 * surge) * crackle;
    rgb += volt * corona;

    let edge = 1.0 - smoothstep(0.85, 1.0, r); // trim before the quad edge
    let alpha = clamp(max(energy, corona) * edge, 0.0, 1.0);

    var color = vec4<f32>(rgb, alpha);
    if ((effects.mask & BRITTLE) != 0u) {
        color = brittle(color, p);
    }
    return color;
}
