#import bevy_sprite::mesh2d_vertex_output::VertexOutput
#import bevy_sprite::mesh2d_view_bindings::globals
#import dwd::voronoi_border::dwd_voronoi_border_2d

// Pure-procedural water wisp: a living droplet of liquid.
//
// A translucent globule with a bright Fresnel rim and a darker, see-through
// core, lit by a roaming specular glint. The silhouette bulges, pinches and
// lunges with surface tension as it swims along its path. Nothing is sampled
// from a texture — every feature is generated from the UV and time.

struct UniformData {
    seed: f32,          // per-instance phase offset, decorrelates a cluster of wisps
    wobble: f32,        // surface-tension silhouette amplitude (at rest)
    flow_speed: f32,    // master animation speed
    tint: f32,          // -1..1 hue shift between deeper blue and brighter teal
    heading_x: f32,     // unit travel direction (quad-local), x
    heading_y: f32,     // unit travel direction (quad-local), y
    vigor: f32,         // measured speed / sweet-spot; the shader derives deform AND cadence from it
    // Oscillator phases are extrapolated from the GPU clock as
    // `anchor_phase + (globals.time - anchor_time) * rate(vigor)`, so the wobble
    // keeps moving every frame without a per-frame upload (the CPU only re-anchors
    // on change). The rate is recomputed from vigor below — it is not stored.
    stroke_anchor_phase: f32, // swim-stroke phase at the anchor instant
    surf_anchor_phase: f32,   // rim-ripple phase at the anchor instant
    anchor_time: f32,         // GPU-clock time the phases were anchored (wrapped seconds)
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

// Mesh padding factor. Mirrors `WispWaterMaterial::QUAD_SCALE` in src/wisps/materials.rs:
// the mesh is built this many times the grid footprint, and we scale UV by it to keep
// the droplet's real size while leaving margin for it to wobble/lunge. Keep equal.
const QUAD_SCALE: f32 = 2.4;

// ── Brittle: the droplet is big enough to carry detail, so it crazes over — a fine
// grid of golden cracks (a Voronoi network) across the whole body, like a frozen,
// fracturing shell. Gold is the complement of the blue, so the network pops; opaque so
// it doesn't wash out through the translucent body. Static, seeded per instance. ─────
const CRACK_DENSITY: f32 = 8.5;   // crackle cell count across the body (higher = finer)
const CRACK_W: f32 = 0.28;        // crack line half-width, in cell units

fn brittle(color: vec4<f32>, q: vec2<f32>, body: f32) -> vec4<f32> {
    let gold = vec3<f32>(1.00, 0.78, 0.25); // golden crack — complement of the blue body

    let md = dwd_voronoi_border_2d(q * CRACK_DENSITY + vec2<f32>(uniforms.seed));
    let crack = (1.0 - smoothstep(0.0, CRACK_W, md)) * body;

    let rgb = mix(color.rgb, gold, crack);
    return vec4<f32>(rgb, max(color.a, crack)); // opaque so the cracks always read
}

// Locomotion: how the droplet deforms while travelling.
// Geometry deform from raw vigor: a saturating curve through (1,1) that asymptotes
// at DEFORM_ASYMP, so the body deforms more with speed but never stretches enough to tear.
const DEFORM_ASYMP: f32 = 1.25;
// Oscillator cadences, radians/sec: rate = rest + swing * vigor. MUST match the
// same-named constants in drive_water_material (src/wisps/systems.rs), which uses
// them for the CPU re-anchor. (Divergence shows up as a phase snap on speed changes.)
const STROKE_RATE_REST: f32 = 3.5;
const STROKE_RATE_SWING: f32 = 3.5;
const SURF_RATE_REST: f32 = 1.5;
const SURF_RATE_SWING: f32 = 6.0;
const STRETCH_BASE: f32 = 0.20; // baseline elongation along heading at full speed
const STRETCH_BEAT: f32 = 0.14; // extra elongation on the forward stroke
const LURCH: f32 = 0.10;        // forward scoot per stroke
const SWAY: f32 = 0.07;         // side-to-side wriggle
const TAIL_TAPER: f32 = 0.22;   // how sharply the trailing edge pinches
const CHURN_AMP_GAIN: f32 = 1.6;// extra rim amplitude at full travel

@fragment
fn fragment(mesh: VertexOutput) -> @location(0) vec4<f32> {
    let t = globals.time * uniforms.flow_speed;
    // Saturating geometry deform from raw vigor: 0 at rest, ≈1 at the sweet spot,
    // asymptotes at DEFORM_ASYMP so the body deforms more with speed but never tears.
    let deform = DEFORM_ASYMP * uniforms.vigor / (uniforms.vigor + DEFORM_ASYMP - 1.0);

    // Travel direction (falls back to a fixed axis while at rest, where deform ≈ 0).
    let heading_raw = vec2<f32>(uniforms.heading_x, uniforms.heading_y);
    let hlen = length(heading_raw);
    let h = select(vec2<f32>(0.0, -1.0), heading_raw / max(hlen, 1e-4), hlen > 1e-3);
    let h_perp = vec2<f32>(-h.y, h.x);

    // Locomotion strokes drive a forward lurch and a lateral wriggle. The cadence
    // is extrapolated from the GPU clock against a CPU-set anchor, so it keeps
    // moving every frame and a speed change re-anchors smoothly (no phase jump).
    let stroke_rate = STROKE_RATE_REST + STROKE_RATE_SWING * uniforms.vigor;
    let stroke = uniforms.stroke_anchor_phase + (globals.time - uniforms.anchor_time) * stroke_rate;
    let beat = sin(stroke + uniforms.seed);
    let lunge = deform * (STRETCH_BASE + STRETCH_BEAT * beat);
    let lurch = deform * LURCH * beat;
    let sway = deform * SWAY * sin(stroke * 0.5 + uniforms.seed * 3.0);

    // Scale the centred UV out into the droplet's own space; the surrounding quad
    // padding then lets the body wobble and lunge without ever clipping the mesh
    // edge. Scoot/wriggle the sample point opposite the motion so the body surges.
    let u = (mesh.uv - vec2<f32>(0.5)) * QUAD_SCALE;
    var p = u - h * lurch - h_perp * sway;

    // Squash & stretch: elongate along the heading, narrow across it (volume-ish).
    let along = dot(p, h);
    let perp = p - h * along;
    let q = h * (along / (1.0 + lunge)) + perp * (1.0 + lunge * 0.5);

    let r = length(q) * 2.0; // 0 at centre, ~1 at the inscribed edge
    let ang = atan2(q.y, q.x);

    // ── Surface-tension silhouette + trailing teardrop taper ──────────────────
    // The rim churns harder and faster the quicker the wisp moves, so the wobble
    // reads as the effort propelling it rather than an idle shimmer.
    let surf_rate = SURF_RATE_REST + SURF_RATE_SWING * uniforms.vigor;
    let surf_t = uniforms.surf_anchor_phase + (globals.time - uniforms.anchor_time) * surf_rate;
    let wob = uniforms.wobble * (1.0 + CHURN_AMP_GAIN * deform);
    let direction_to_edge = q / max(length(q), 1e-4);
    let trailing = max(0.0, -dot(direction_to_edge, h)); // 1 on the trailing side
    var edge = 0.9
        + wob * sin(ang * 3.0 + surf_t + uniforms.seed)
        + wob * 0.55 * sin(ang * 5.0 - surf_t * 1.3 + uniforms.seed * 2.0)
        + wob * 0.35 * deform * sin(ang * 8.0 + surf_t * 1.9); // choppy crests at speed
    edge *= 1.0 - TAIL_TAPER * deform * trailing; // pinch the wake into a tail
    let body = smoothstep(edge, edge - 0.14, r);
    if (body <= 0.0) {
        return vec4<f32>(0.0);
    }

    // ── Fresnel: translucent core, bright silhouette ──────────────────────────
    let fres = pow(clamp(r / edge, 0.0, 1.0), 3.0);

    // ── Aquatic gradient: deep blue core → teal → foam-white rim ──────────────
    let deep = vec3<f32>(0.02, 0.12, 0.45);
    let teal = vec3<f32>(0.08, 0.52, 0.80);
    let foam = vec3<f32>(0.75, 0.95, 1.00);
    var col = mix(deep, teal, smoothstep(0.0, 0.75, r / edge));
    col = mix(col, foam, fres * 0.85);
    col += vec3<f32>(-0.02, 0.03, 0.04) * uniforms.tint; // per-instance hue shift

    // ── Core pulses brighter on each forward stroke — energy, not lines ───────
    col += foam * 0.06 * body * (1.0 - fres) * deform * (0.5 + 0.5 * beat);

    // ── Roaming specular glint, biased toward the leading edge ────────────────
    let glint_pos = h * (0.18 * deform) + vec2<f32>(
        cos(t * 0.7 + uniforms.seed),
        sin(t * 0.9 + uniforms.seed * 1.7),
    ) * 0.22;
    let glint = smoothstep(0.16, 0.0, length(q - glint_pos));
    col += foam * glint * 0.7 * body;

    // ── Alpha: see-through core, denser rim, opaque highlights ────────────────
    var alpha = body * (0.55 + 0.45 * fres);
    alpha = max(alpha, glint * body);
    alpha = clamp(alpha, 0.0, 1.0);

    var color = vec4<f32>(col, alpha);
    if ((effects.mask & BRITTLE) != 0u) {
        color = brittle(color, q, body);
    }
    return color;
}
