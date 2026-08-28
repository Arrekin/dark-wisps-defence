#import bevy_sprite::mesh2d_vertex_output::VertexOutput
#import bevy_sprite::mesh2d_view_bindings::globals
#import dwd::core::TAU
#import dwd::voronoi_border::{dwd_voronoi_border_2d, dwd_voronoi_hash_2d}

// Pure-procedural light wisp: a radiant prism-star.
//
// A brilliant white core throws soft, slowly-rotating beams wrapped in a gentle
// bloom; the beams whiten at the core and split into a pale spectrum toward their
// tips — white light through a prism, the wisp's "every colour at once" signature
// (an angular dispersion fan, distinct from the quantum field's chromatic rim). A
// crisp star glint sits over the core. Travelling brightens and flares it and
// sheds tiny spectral motes into its wake. Nothing is sampled from a texture.

struct UniformData {
    seed: f32,      // per-instance phase offset, decorrelates a cluster of wisps
    vigor: f32,     // measured speed / sweet-spot; brightens, flares + sheds motes (0 at rest)
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

// ── Radiance tuning ──────────────────────────────────────────────────────────
const RAYS_PRIMARY: f32 = 7.0;    // long beam count
const RAYS_SECONDARY: f32 = 13.0; // fine beam count
const RAY_SHARP: f32 = 3.0;       // beam thinness (higher = thinner)
const ROT_SPEED: f32 = 0.25;      // slow beam rotation
const PULSE_SPEED: f32 = 2.0;     // gentle breathing
const BLOOM_FALLOFF: f32 = 3.5;   // soft halo size (higher = tighter, less white wash)
const SPECTRUM_SAT: f32 = 0.70;   // how vivid the prism dispersion gets

// ── Motion: while travelling, the star sheds tiny glinting motes into its wake ─
const NUM_MOTES: i32 = 8;
const MOTE_REACH: f32 = 0.9;  // how far a mote drifts before fading
const MOTE_SIZE: f32 = 0.05;  // mote radius
const MOTE_RATE: f32 = 0.8;   // shed cadence
const MOTE_SPREAD: f32 = 0.4; // perpendicular fan width

// Smooth spectral palette (cosine palette): t → a hue around the full spectrum.
fn spectrum(t: f32) -> vec3<f32> {
    return vec3<f32>(0.5) + vec3<f32>(0.5) * cos(TAU * (t + vec3<f32>(0.00, 0.33, 0.67)));
}

// ── Brittle: a cage of fine golden cracks over the core — a dense crackle (a Voronoi
// network) filling a disc, like a 3D cage wrapping the star's core, held in near it
// (not out in the bloom). The wisps are too small for bold seams, so the crackle
// texture is the tell. Black, to read against the white glare. Static, seeded. ───────
const CRACK_DENSITY: f32 = 12.0;  // crackle cell count (higher = finer cracks)
const CRACK_W: f32 = 0.50;        // crack line half-width, in cell units
const CAGE_R: f32 = 0.28;         // cage disc radius (r = length(p)*2 space, covers the core)

fn brittle(color: vec4<f32>, p: vec2<f32>) -> vec4<f32> {
    let cage_col = vec3<f32>(0.0, 0.0, 0.0); // black cage — reads against the white glare

    let r = length(p) * 2.0;
    let band = 1.0 - smoothstep(CAGE_R * 0.8, CAGE_R, r); // filled disc over the core
    let md = dwd_voronoi_border_2d(p * CRACK_DENSITY + vec2<f32>(uniforms.seed));
    let crack = (1.0 - smoothstep(0.0, CRACK_W, md)) * band;

    let rgb = mix(color.rgb, cage_col, crack);
    return vec4<f32>(rgb, max(color.a, crack)); // opaque so the cage always reads
}

@fragment
fn fragment(mesh: VertexOutput) -> @location(0) vec4<f32> {
    let p = mesh.uv - vec2<f32>(0.5);
    let r = length(p) * 2.0; // 0 at centre, ~1 at the inscribed edge
    let ang = atan2(p.y, p.x);
    let vigor = uniforms.vigor;
    let pulse = 0.85 + 0.15 * sin(globals.time * PULSE_SPEED + uniforms.seed);

    // ── Slowly-rotating god-ray beams (fixed geometry; speed only brightens) ──
    let spin = globals.time * ROT_SPEED + uniforms.seed;
    let beams = pow(0.5 + 0.5 * sin((ang + spin) * RAYS_PRIMARY), RAY_SHARP) * 0.7
              + pow(0.5 + 0.5 * sin((ang - spin * 0.6) * RAYS_SECONDARY + 1.3), RAY_SHARP) * 0.3;
    let reach = 0.55; // constant — the star geometry never changes with speed
    let rays = beams * exp(-r / reach) * smoothstep(0.0, 0.1, r) * pulse * (1.0 + 0.3 * vigor);

    // Prismatic dispersion: the beams are white at the core and split into a vivid
    // spectrum from close in — where they are still bright — so the colour reads.
    // An angular prism fan, deliberately unlike the quantum field's chromatic rim.
    let hue = ang / TAU + globals.time * 0.04;
    let dispersion = SPECTRUM_SAT * smoothstep(0.08, 0.4, r);
    let ray_color = mix(vec3<f32>(1.0), spectrum(hue), dispersion);

    // ── Brilliant white core + crisp 4-point star glint ───────────────────────
    let core = smoothstep(0.16, 0.0, r) * pulse;
    // The glint flares brighter and reaches further with speed — along the same
    // two axes, so the star is intensified, not reshaped.
    let flare = 1.0 + 1.2 * vigor;
    let spike_reach = 0.6 + 0.3 * vigor;
    let sparkle = (pow(max(0.0, 1.0 - abs(p.y) * 30.0), 2.0)
                 + pow(max(0.0, 1.0 - abs(p.x) * 30.0), 2.0)) * smoothstep(spike_reach, 0.0, r) * flare;

    // ── Soft white bloom — brightens with speed, geometry unchanged ───────────
    let bloom = exp(-r * BLOOM_FALLOFF) * (0.4 + 0.4 * vigor);

    // ── Motion as a separate layer: shed tiny glinting motes into the wake. The
    //    star itself is untouched — speed only adds these motes and a little glow.
    let heading = vec2<f32>(uniforms.heading_x, uniforms.heading_y);
    let wake = -heading;                          // opposite to travel
    let wake_perp = vec2<f32>(-wake.y, wake.x);
    var motes = 0.0;                // scalar coverage (drives alpha/energy)
    var motes_rgb = vec3<f32>(0.0); // each mote carries its own spectral tint
    for (var i = 0; i < NUM_MOTES; i = i + 1) {
        let rnd = dwd_voronoi_hash_2d(vec2<f32>(f32(i), uniforms.seed)); // per-mote randoms
        let life = fract(globals.time * MOTE_RATE + rnd.x);     // staggered 0..1 lifetime
        let spread = (rnd.y - 0.5) * MOTE_SPREAD * life;        // fan out as it ages
        let pos = wake * (life * MOTE_REACH) + wake_perp * spread;
        let glow = (1.0 - smoothstep(0.0, MOTE_SIZE, length(p - pos))) * (1.0 - life);
        motes += glow;
        motes_rgb += spectrum(rnd.x + life * 0.2) * glow; // own hue, drifting as it ages
    }
    motes *= vigor; // only while travelling
    motes_rgb *= vigor;

    // ── Compose: white core/glint/bloom, spectral rays, spectral motes ────────
    let white = vec3<f32>(1.0);
    let energy = core + sparkle * 0.8 + bloom + rays + motes;
    var rgb = white * (core + sparkle * 0.8 + bloom)
            + ray_color * rays
            + motes_rgb;

    let edge = 1.0 - smoothstep(0.7, 0.98, r); // trim before the quad edge — no clipping
    let alpha = clamp(energy * edge, 0.0, 1.0);

    var color = vec4<f32>(rgb, alpha);
    if ((effects.mask & BRITTLE) != 0u) {
        color = brittle(color, p);
    }
    return color;
}
