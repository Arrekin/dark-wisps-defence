#import bevy_core_pipeline::fullscreen_vertex_shader::FullscreenVertexOutput

// Quantum field anomaly. Screen-space pass over the already-rendered frame, so it can
// distort / ghost the walls, wisps and towers sitting on top of a field's rectangle.
// See tasks.md for the design + the GPU data contract.

@group(0) @binding(0) var screen_texture: texture_2d<f32>;
@group(0) @binding(1) var screen_sampler: sampler;
@group(0) @binding(2) var<uniform> camera: CameraData;
@group(0) @binding(3) var<storage, read> fields: array<QuantumFieldEntry>;
// @binding(4) RESERVED for a future scan-sweep buffer (see tasks.md §6).

struct CameraData {
    world_pos:     vec2<f32>,
    viewport_size: vec2<f32>,
    global_time:   f32,
    field_count:   u32,
}

struct QuantumFieldEntry {
    center:         vec2<f32>,
    half_extent:    vec2<f32>,
    solve_progress: f32, // 0 = fresh anomaly, 1 = solved
    seed:           f32, // per-field noise offset
    scan_activity:  f32, // RESERVED — always 0 for now (future sweep-collapse)
    _pad:           f32,
}

// ── Effect switches (compile-time; edit + restart the app to apply) ───────────
const ENABLE_BOUNDARY:             bool = true;
const ENABLE_BOUNDARY_UNCERTAINTY: bool = true;
const ENABLE_MOIRE:                bool = true;   // moiré lattice — the interior's visible structure
const ENABLE_INTERFERENCE:         bool = true;   // drifting standing-wave brightness bands across the lattice
const ENABLE_SCHLIEREN:            bool = true;
const ENABLE_SUPERPOSITION:        bool = true;
const ENABLE_DECOHERENCE:          bool = true;
const ENABLE_PHASE_FLICKER:        bool = true;
const ENABLE_LAYER_DIMINISH:       bool = true;
const ENABLE_SCAN_COLLAPSE:        bool = false; // RESERVED — see tasks.md §6

// ── Palette ───────────────────────────────────────────────────────────────────
const COLOR_BASE:     vec3<f32> = vec3<f32>(0.45, 0.20, 0.85); // violet interior
const COLOR_ACCENT:   vec3<f32> = vec3<f32>(0.25, 1.00, 0.70); // cyan-green spark on bright lattice nodes
const COLOR_BOUNDARY: vec3<f32> = vec3<f32>(0.60, 0.45, 1.00); // brighter violet rim — matches the interior

// ── Boundary tunables (world units unless noted) ──────────────────────────────
const BOUNDARY_THICKNESS:   f32 = 2.5;   // visible rim line half-width (thinner than before)
const INTERIOR_FADE:        f32 = 6.0;   // how far inside the interior ramps to full strength;
                                         // also governs object-effect edge falloff (unchanged behavior)
const BOUNDARY_OUTER_BAND:  f32 = 8.0;   // distance past the edge still processed
const BOUNDARY_UNCERT_AMP:  f32 = 5.0;   // edge jitter amplitude
const BOUNDARY_UNCERT_FREQ: f32 = 0.05;  // edge jitter spatial frequency

// ── Interior pattern tunables ─────────────────────────────────────────────────
// Moiré lattice — two beating grids, gently warped by a curl flow so the mesh writhes.
const MOIRE_SCALE_A:       f32 = 0.10;
const MOIRE_SCALE_B:       f32 = 0.115;  // slightly different scale → beat pattern
const MOIRE_ROT:           f32 = 0.20;   // radians between the two lattices
const MOIRE_INTENSITY:     f32 = 0.60;   // overall lattice brightness
const ACCENT_AMOUNT:       f32 = 0.20;   // cyan-green spark on the brightest lattice nodes (0 = none)
const MOIRE_WARP_SCALE:    f32 = 0.0275; // curl-warp frequency (how the mesh writhes)
const MOIRE_WARP_SPEED:    f32 = 0.30;   // curl-warp drift speed
const MOIRE_WARP_STRENGTH: f32 = 0.6;    // how much the mesh writhes (0 = static grid)
// Interference — drifting standing-wave bands that modulate the lattice brightness.
const INTERFERENCE_SCALE:  f32 = 0.05;   // bands ≈ half_extent * scale
const INTERFERENCE_SPEED:  f32 = 1.9;
// Schlieren — refraction of the underlying frame (object effect).
const SCHLIEREN_SCALE:     f32 = 0.05;
const SCHLIEREN_STRENGTH:  f32 = 8.0;

// ── Object-effect tunables ────────────────────────────────────────────────────
const SUPERPOSITION_TAPS:   i32 = 4;
const SUPERPOSITION_OFFSET: f32 = 8.0;   // max ghost spread
const SUPERPOSITION_SPIN:   f32 = 0.7;   // rad/s rotation of the ghost cluster
const DECOHERENCE_SPLIT:    f32 = 4.0;   // channel separation
const PHASE_FLICKER_SPEED:  f32 = 9.0;
const PHASE_FLICKER_DEPTH:  f32 = 0.6;   // how strongly the rim dims (0..1)

const TAU: f32 = 6.28318530718;

// ── Coordinate helpers (match the other post-process passes) ──────────────────
fn uv_to_world(uv: vec2<f32>) -> vec2<f32> {
    let centered = uv - vec2<f32>(0.5, 0.5);
    return camera.world_pos + centered * camera.viewport_size * vec2<f32>(1.0, -1.0);
}
fn world_to_uv(world: vec2<f32>) -> vec2<f32> {
    let centered = (world - camera.world_pos) / camera.viewport_size;
    return centered * vec2<f32>(1.0, -1.0) + vec2<f32>(0.5, 0.5);
}

fn sample_rgb(uv: vec2<f32>) -> vec3<f32> {
    return textureSampleLevel(screen_texture, screen_sampler, uv, 0.0).rgb;
}

// ── Noise (borrowed from force_field_post_process.wgsl) ───────────────────────
fn hash(p: vec2<f32>) -> f32 {
    let h = dot(p, vec2<f32>(127.1, 311.7));
    return fract(sin(h) * 43758.5453123);
}
fn noise(p: vec2<f32>) -> f32 {
    let i = floor(p);
    let f = fract(p);
    let a = hash(i);
    let b = hash(i + vec2<f32>(1.0, 0.0));
    let c = hash(i + vec2<f32>(0.0, 1.0));
    let d = hash(i + vec2<f32>(1.0, 1.0));
    let u = f * f * (3.0 - 2.0 * f);
    return mix(mix(a, b, u.x), mix(c, d, u.x), u.y);
}
fn fbm(p: vec2<f32>) -> f32 {
    return noise(p) * 0.6 + noise(p * 2.1 + vec2<f32>(4.3, 1.7)) * 0.4;
}
// Divergence-free 2D curl of a noise potential — a flow velocity (~[-1,1]) that rolls along
// streamlines instead of drifting, so the chaos churns like turbulence. (From the force field.)
fn curl(p: vec2<f32>) -> vec2<f32> {
    let e = 0.5;
    return vec2<f32>(
         noise(p + vec2<f32>(0.0,  e)) - noise(p - vec2<f32>(0.0,  e)),
        -(noise(p + vec2<f32>(e, 0.0)) - noise(p - vec2<f32>(e, 0.0)))
    );
}
// ── Geometry helpers ──────────────────────────────────────────────────────────
// Signed distance to an axis-aligned box. Negative inside, 0 on the edge, positive outside.
fn box_sdf(p: vec2<f32>, half: vec2<f32>) -> f32 {
    let d = abs(p) - half;
    return length(max(d, vec2<f32>(0.0))) + min(max(d.x, d.y), 0.0);
}
fn rotate(v: vec2<f32>, a: f32) -> vec2<f32> {
    let c = cos(a);
    let s = sin(a);
    return vec2<f32>(c * v.x - s * v.y, s * v.x + c * v.y);
}
// 0..1 lattice of soft dots; two of these at slightly different scale/rotation beat into moiré.
fn grid_wave(p: vec2<f32>) -> f32 {
    return (sin(p.x * TAU) * 0.5 + 0.5) * (sin(p.y * TAU) * 0.5 + 0.5);
}

@fragment
fn fragment(in: FullscreenVertexOutput) -> @location(0) vec4<f32> {
    let count = camera.field_count;
    if count == 0u {
        return textureSampleLevel(screen_texture, screen_sampler, in.uv, 0.0);
    }

    let world_pos = uv_to_world(in.uv);
    let t = camera.global_time;

    // Fields never overlap (placement validator), so the deepest box (smallest SDF) wins.
    var best_sdf = 1.0e9;
    var best_idx = 0u;
    for (var i = 0u; i < count; i++) {
        let sdf = box_sdf(world_pos - fields[i].center, fields[i].half_extent);
        if sdf < best_sdf {
            best_sdf = sdf;
            best_idx = i;
        }
    }
    let field = fields[best_idx];

    // Uncertain boundary: wobble the effective edge used for masking.
    var edge_sdf = best_sdf;
    if ENABLE_BOUNDARY_UNCERTAINTY {
        let jitter = fbm(world_pos * BOUNDARY_UNCERT_FREQ + vec2<f32>(field.seed, t * 0.3)) - 0.5;
        edge_sdf += jitter * 2.0 * BOUNDARY_UNCERT_AMP;
    }

    // Anything well outside the (possibly wobbled) edge is untouched.
    if edge_sdf > BOUNDARY_OUTER_BAND {
        return textureSampleLevel(screen_texture, screen_sampler, in.uv, 0.0);
    }

    let weirdness = select(1.0, 1.0 - field.solve_progress, ENABLE_LAYER_DIMINISH);
    // interior: object-effect region (uses INTERIOR_FADE → behavior unchanged from before).
    let interior  = 1.0 - smoothstep(-INTERIOR_FADE, 0.0, edge_sdf); // 1 inside → 0 at edge
    // rim_zone: same ~6u band the phase-flicker used previously (kept independent of the line).
    let rim_zone  = 1.0 - smoothstep(0.0, INTERIOR_FADE, abs(edge_sdf));
    // on_edge: the *visible* boundary line — now thin (BOUNDARY_THICKNESS).
    let on_edge   = 1.0 - smoothstep(0.0, BOUNDARY_THICKNESS, abs(edge_sdf));
    let to_center = world_pos - field.center;

    // ── Build the world-space coordinate we sample the frame from ──────────────
    var sample_world = world_pos;

    if ENABLE_SCHLIEREN {
        let nx = fbm(world_pos * SCHLIEREN_SCALE + vec2<f32>(t * 0.20 + field.seed, 0.0));
        let ny = fbm(world_pos * SCHLIEREN_SCALE + vec2<f32>(0.0, t * 0.17 + field.seed));
        sample_world += (vec2<f32>(nx, ny) - 0.5) * 2.0 * SCHLIEREN_STRENGTH * weirdness * interior;
    }

    // ── Sample the frame: superposition ghosting + chromatic decoherence ───────
    var rgb: vec3<f32>;
    if ENABLE_SUPERPOSITION {
        let spread = SUPERPOSITION_OFFSET * weirdness * interior;
        var acc  = sample_rgb(world_to_uv(sample_world)) * 2.0; // real position weighted higher
        var wsum = 2.0;
        for (var k = 0; k < SUPERPOSITION_TAPS; k++) {
            let ang = TAU * f32(k) / f32(SUPERPOSITION_TAPS) + t * SUPERPOSITION_SPIN;
            let off = vec2<f32>(cos(ang), sin(ang)) * spread;
            acc += sample_rgb(world_to_uv(sample_world + off));
            wsum += 1.0;
        }
        rgb = acc / wsum;
    } else {
        rgb = sample_rgb(world_to_uv(sample_world));
    }
    if ENABLE_DECOHERENCE {
        let split = DECOHERENCE_SPLIT * weirdness * interior;
        let len   = length(to_center);
        let dir   = select(vec2<f32>(1.0, 0.0), to_center / max(len, 0.001), len > 0.001);
        let cr = sample_rgb(world_to_uv(sample_world + dir * split)).r;
        let cb = sample_rgb(world_to_uv(sample_world - dir * split)).b;
        rgb = vec3<f32>(cr, rgb.g, cb);
    }

    // ── Phase flicker near the rim (screen-space approximation of tunneling) ───
    if ENABLE_PHASE_FLICKER {
        let f = fbm(world_pos * 0.2 + vec2<f32>(t * PHASE_FLICKER_SPEED, field.seed));
        rgb *= 1.0 - PHASE_FLICKER_DEPTH * rim_zone * weirdness * step(0.5, f);
    }

    // ── Interior: a warped, drifting moiré lattice with interference bands ─────
    // Two slightly-different lattices beat into a moiré; a gentle curl warp makes the mesh
    // writhe and drift instead of sitting as a static grid. Interference adds slow standing-
    // wave brightness bands sweeping across it.

    // Interference → standing-wave brightness multiplier (drifting bands).
    var bright = 1.0;
    if ENABLE_INTERFERENCE {
        let w1 = sin(dot(to_center, vec2<f32>(1.0, 0.3))  * INTERFERENCE_SCALE * TAU + t * INTERFERENCE_SPEED);
        let w2 = sin(dot(to_center, vec2<f32>(-0.4, 1.0)) * INTERFERENCE_SCALE * TAU - t * INTERFERENCE_SPEED * 0.8);
        bright = 0.55 + 0.45 * (w1 + w2) * 0.5; // ~0.1 .. 1.0
    }

    // Warped moiré lattice: two beating grids, gently warped by a curl flow so the mesh writhes.
    var lattice = 0.0;
    if ENABLE_MOIRE {
        let warp = curl(to_center * MOIRE_WARP_SCALE + vec2<f32>(t * MOIRE_WARP_SPEED + field.seed, 0.0)) * MOIRE_WARP_STRENGTH;
        let la = grid_wave(to_center * MOIRE_SCALE_A + warp + vec2<f32>(t * 0.05, 0.0));
        let lb = grid_wave(rotate(to_center, MOIRE_ROT) * MOIRE_SCALE_B + warp);
        lattice = la * lb;
    }

    // Compose: violet lattice lit by the interference bands, faint cyan-green spark on bright nodes.
    let lit = lattice * bright;
    var overlay = COLOR_BASE   * lit * MOIRE_INTENSITY
                + COLOR_ACCENT * pow(clamp(lit, 0.0, 1.0), 4.0) * ACCENT_AMOUNT;
    overlay *= interior * weirdness;

    // ── Boundary ───────────────────────────────────────────────────────────────
    // Violet rim matching the interior style — just bright enough to delineate the area.
    var boundary_col = vec3<f32>(0.0);
    if ENABLE_BOUNDARY {
        boundary_col += COLOR_BOUNDARY * on_edge * (0.6 + 0.4 * weirdness);
    }

    // Faint interior tint so the area still reads when calm/solved.
    let base_tint = COLOR_BASE * interior * (0.05 + 0.10 * weirdness);

    var result = rgb + overlay + boundary_col + base_tint;

    // ── RESERVED: scan-collapse (see tasks.md §6) ──────────────────────────────
    if ENABLE_SCAN_COLLAPSE {
        // TODO(scan-collapse): drive from ExpeditionZone / drone state via field.scan_activity.
        // Would crisp the lattice and snap superposition ghosts where the sweep passes.
        // Inert until a future system writes scan_activity (> 0); referenced so the binding stays live.
        result += COLOR_ACCENT * 0.0 * field.scan_activity;
    }

    return vec4<f32>(result, 1.0);
}
