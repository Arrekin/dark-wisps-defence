#import bevy_core_pipeline::fullscreen_vertex_shader::FullscreenVertexOutput
#import dwd::core::TAU
#import dwd::value_noise::{dwd_value_fbm_2d, dwd_value_noise_curl_2d}

// Quantum field anomaly. Screen-space pass over the already-rendered frame, so it can
// distort / ghost the walls, wisps and towers sitting on top of a field's rectangle.
// See documentation/post_process_effects.md for the architecture + GPU data contract.

@group(0) @binding(0) var screen_texture: texture_2d<f32>;
@group(0) @binding(1) var screen_sampler: sampler;
@group(0) @binding(2) var<uniform> camera: CameraData;
@group(0) @binding(3) var<storage, read> fields: array<QuantumFieldEntry>;
@group(0) @binding(4) var<storage, read> collapse_points: array<CollapsePoint>;

struct CameraData {
    world_pos:      vec2<f32>,
    viewport_size:  vec2<f32>,
    global_time:    f32,
    field_count:    u32,
    collapse_count: u32, // active scan-collapse points (the storage buffer never shrinks, so we
                         // can't trust arrayLength — loop on this explicit count instead)
}

struct QuantumFieldEntry {
    center:         vec2<f32>,
    half_extent:    vec2<f32>,
    solve_progress: f32, // 0 = fresh anomaly, 1 = solved
    seed:           f32, // per-field noise offset
}

struct CollapsePoint {
    pos: vec2<f32>, // scan-beam ground spot (world space)
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
const ENABLE_SCAN_COLLAPSE:        bool = true;  // drone beams locally calm the field around their scan spot

// ── Palette ───────────────────────────────────────────────────────────────────
const COLOR_BASE:     vec3<f32> = vec3<f32>(0.45, 0.20, 0.85); // violet interior
const COLOR_ACCENT:   vec3<f32> = vec3<f32>(0.25, 1.00, 0.70); // cyan-green spark on bright lattice nodes
const COLOR_BOUNDARY: vec3<f32> = vec3<f32>(0.60, 0.45, 1.00); // brighter violet rim — matches the interior

// ── Boundary tunables (world units unless noted) ──────────────────────────────
const BOUNDARY_THICKNESS:   f32 = 2.5;   // visible rim line half-width
const INTERIOR_FADE:        f32 = 6.0;   // distance the interior ramps to full strength over;
                                         // also the object-effect edge falloff distance
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

// ── Scan-collapse — drone beams locally calm the field around their ground spot ──────────
const SCAN_COLLAPSE_RADIUS: f32 = 40.0;  // world-unit radius of the calmed region per scan spot
const SCAN_GLOW:           f32 = 0.5;    // brightness of the soft "observed" rim


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
        let jitter = dwd_value_fbm_2d(world_pos * BOUNDARY_UNCERT_FREQ + vec2<f32>(field.seed, t * 0.3)) - 0.5;
        edge_sdf += jitter * 2.0 * BOUNDARY_UNCERT_AMP;
    }

    // Anything well outside the (possibly wobbled) edge is untouched.
    if edge_sdf > BOUNDARY_OUTER_BAND {
        return textureSampleLevel(screen_texture, screen_sampler, in.uv, 0.0);
    }

    let weirdness = select(1.0, 1.0 - field.solve_progress, ENABLE_LAYER_DIMINISH);
    // interior: 1 deep inside → 0 at the edge. Masks the interior pattern and the object effects.
    let interior  = 1.0 - smoothstep(-INTERIOR_FADE, 0.0, edge_sdf);
    // rim_zone: a band straddling the edge (INTERIOR_FADE wide) — drives the phase-flicker.
    let rim_zone  = 1.0 - smoothstep(0.0, INTERIOR_FADE, abs(edge_sdf));
    // on_edge: the thin visible boundary line (BOUNDARY_THICKNESS wide).
    let on_edge   = 1.0 - smoothstep(0.0, BOUNDARY_THICKNESS, abs(edge_sdf));
    let to_center = world_pos - field.center;

    // ── Scan-collapse: where a drone beam observes the field, the anomaly calms locally ──
    // For each active scan spot, calm a disc around it: superposition + decoherence snap toward
    // the real object and the chaotic lattice eases (observation collapses the wavefunction).
    var collapse = 0.0;
    if ENABLE_SCAN_COLLAPSE {
        for (var i = 0u; i < camera.collapse_count; i++) {
            let d = distance(world_pos, collapse_points[i].pos);
            collapse = max(collapse, smoothstep(SCAN_COLLAPSE_RADIUS, SCAN_COLLAPSE_RADIUS * 0.35, d));
        }
        collapse *= interior;
    }

    // ── Build the world-space coordinate we sample the frame from ──────────────
    var sample_world = world_pos;

    if ENABLE_SCHLIEREN {
        let nx = dwd_value_fbm_2d(world_pos * SCHLIEREN_SCALE + vec2<f32>(t * 0.20 + field.seed, 0.0));
        let ny = dwd_value_fbm_2d(world_pos * SCHLIEREN_SCALE + vec2<f32>(0.0, t * 0.17 + field.seed));
        sample_world += (vec2<f32>(nx, ny) - 0.5) * 2.0 * SCHLIEREN_STRENGTH * weirdness * interior;
    }
    // Inside a collapse disc, pin sampling back to the true position — schlieren and any other
    // displacement vanish, so the observed area shows crystal-clear reality (rim still distorts).
    sample_world = mix(sample_world, world_pos, collapse);

    // ── Sample the frame: superposition ghosting + chromatic decoherence ───────
    var rgb: vec3<f32>;
    if ENABLE_SUPERPOSITION {
        let spread = SUPERPOSITION_OFFSET * weirdness * interior * (1.0 - collapse);
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
        let split = DECOHERENCE_SPLIT * weirdness * interior * (1.0 - collapse);
        let len   = length(to_center);
        let dir   = select(vec2<f32>(1.0, 0.0), to_center / max(len, 0.001), len > 0.001);
        let cr = sample_rgb(world_to_uv(sample_world + dir * split)).r;
        let cb = sample_rgb(world_to_uv(sample_world - dir * split)).b;
        rgb = vec3<f32>(cr, rgb.g, cb);
    }

    // ── Phase flicker near the rim (screen-space approximation of tunneling) ───
    if ENABLE_PHASE_FLICKER {
        let f = dwd_value_fbm_2d(world_pos * 0.2 + vec2<f32>(t * PHASE_FLICKER_SPEED, field.seed));
        rgb *= 1.0 - PHASE_FLICKER_DEPTH * rim_zone * weirdness * step(0.5, f) * (1.0 - collapse);
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
        let warp = dwd_value_noise_curl_2d(to_center * MOIRE_WARP_SCALE + vec2<f32>(t * MOIRE_WARP_SPEED + field.seed, 0.0)) * MOIRE_WARP_STRENGTH;
        let la = grid_wave(to_center * MOIRE_SCALE_A + warp + vec2<f32>(t * 0.05, 0.0));
        let lb = grid_wave(rotate(to_center, MOIRE_ROT) * MOIRE_SCALE_B + warp);
        lattice = la * lb;
    }

    // Compose: violet lattice lit by the interference bands, faint cyan-green spark on bright nodes.
    let lit = lattice * bright;
    var overlay = COLOR_BASE   * lit * MOIRE_INTENSITY
                + COLOR_ACCENT * pow(clamp(lit, 0.0, 1.0), 4.0) * ACCENT_AMOUNT;
    // Scan-collapse eases the chaotic lattice toward calm where the field is being observed.
    overlay *= interior * weirdness * (1.0 - collapse);

    // ── Boundary ───────────────────────────────────────────────────────────────
    // Violet rim matching the interior style — just bright enough to delineate the area.
    var boundary_col = vec3<f32>(0.0);
    if ENABLE_BOUNDARY {
        boundary_col += COLOR_BOUNDARY * on_edge * (0.6 + 0.4 * weirdness);
    }

    // Faint interior tint so the area still reads when calm/solved (also eased by collapse).
    let base_tint = COLOR_BASE * interior * (0.05 + 0.10 * weirdness) * (1.0 - collapse);

    var result = rgb + overlay + boundary_col + base_tint;

    // Scan-collapse: soft violet rim marking each observed/calmed disc (0 when nothing scanning).
    let collapse_rim = collapse * (1.0 - collapse) * 4.0; // peaks at the disc edge
    result += COLOR_BOUNDARY * collapse_rim * SCAN_GLOW;

    return vec4<f32>(result, 1.0);
}
