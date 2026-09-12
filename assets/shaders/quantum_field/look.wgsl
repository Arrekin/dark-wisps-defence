#define_import_path dwd::quantum_field

// Shared procedural glow for map and UI rendering: boundary, interior tint, and moiré lattice.
// Frame-sampling distortion remains in quantum_field_post_process.wgsl.

#import dwd::core::TAU
#import dwd::value_noise::{dwd_value_fbm_2d, dwd_value_noise_curl_2d}

// ── Effect switches ──────────────────────────────────────────────────────────
const ENABLE_BOUNDARY:             bool = true;
const ENABLE_BOUNDARY_UNCERTAINTY: bool = true;
const ENABLE_MOIRE:                bool = true;   // moiré lattice — the interior's visible structure
const ENABLE_INTERFERENCE:         bool = true;   // drifting standing-wave brightness bands across the lattice
const ENABLE_LAYER_DIMINISH:       bool = true;

// ── Palette ───────────────────────────────────────────────────────────────────
const COLOR_BASE:     vec3<f32> = vec3<f32>(0.45, 0.20, 0.85); // violet interior
const COLOR_ACCENT:   vec3<f32> = vec3<f32>(0.25, 1.00, 0.70); // cyan-green spark on bright lattice nodes
const COLOR_BOUNDARY: vec3<f32> = vec3<f32>(0.60, 0.45, 1.00); // brighter violet rim — matches the interior

const BOUNDARY_THICKNESS:   f32 = 2.5;   // visible rim line half-width
const INTERIOR_FADE:        f32 = 6.0;   // distance the interior ramps to full strength over;
                                         // also the object-effect edge falloff distance
const BOUNDARY_UNCERT_AMP:  f32 = 5.0;   // edge jitter amplitude
const BOUNDARY_UNCERT_FREQ: f32 = 0.05;  // edge jitter spatial frequency

// ── Interior pattern tunables ─────────────────────────────────────────────────
// Moiré lattice from two differently scaled grids warped by curl noise.
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

// The masks that shape a field's look at one point.
struct QuantumFieldMasks {
    // Signed distance to the (jittered) edge, world pixels. Negative inside.
    edge_sdf: f32,
    // 1 for a fresh anomaly, 0 for a solved one.
    weirdness: f32,
    // 1 deep inside, 0 at the edge.
    interior: f32,
    // A band straddling the edge, INTERIOR_FADE wide.
    rim_zone: f32,
    // The thin visible boundary line, BOUNDARY_THICKNESS wide.
    on_edge: f32,
}

// Computes the masks for one point in a field. `local` is that point relative to the field's
// centre. `noise_at` is where the edge jitter is sampled: on the map that is the world position,
// which keeps two neighbouring fields from wobbling in step. Both in world pixels.
fn dwd_quantum_field_masks(
    local: vec2<f32>,
    half_extent: vec2<f32>,
    noise_at: vec2<f32>,
    time: f32,
    seed: f32,
    solve_progress: f32,
) -> QuantumFieldMasks {
    // Uncertain boundary: wobble the effective edge used for masking.
    var edge_sdf = box_sdf(local, half_extent);
    if ENABLE_BOUNDARY_UNCERTAINTY {
        let jitter = dwd_value_fbm_2d(noise_at * BOUNDARY_UNCERT_FREQ + vec2<f32>(seed, time * 0.3)) - 0.5;
        edge_sdf += jitter * 2.0 * BOUNDARY_UNCERT_AMP;
    }

    let weirdness = select(1.0, 1.0 - solve_progress, ENABLE_LAYER_DIMINISH);
    // Interior mask: 1 inside, fading to 0 at the edge.
    let interior  = 1.0 - smoothstep(-INTERIOR_FADE, 0.0, edge_sdf);
    // Edge band used by phase flicker.
    let rim_zone  = 1.0 - smoothstep(0.0, INTERIOR_FADE, abs(edge_sdf));
    // Boundary-line mask.
    let on_edge   = 1.0 - smoothstep(0.0, BOUNDARY_THICKNESS, abs(edge_sdf));

    return QuantumFieldMasks(edge_sdf, weirdness, interior, rim_zone, on_edge);
}

// The light a field adds on top of whatever is behind it: the moiré interior, the boundary line
// and the faint interior tint. `collapse` runs 0..1 and eases all of it toward calm — a drone
// beam scanning the field raises it.
fn dwd_quantum_field_glow(
    local: vec2<f32>,
    masks: QuantumFieldMasks,
    time: f32,
    seed: f32,
    collapse: f32,
) -> vec3<f32> {
    // Drifting standing-wave brightness modulation.
    var bright = 1.0;
    if ENABLE_INTERFERENCE {
        let w1 = sin(dot(local, vec2<f32>(1.0, 0.3))  * INTERFERENCE_SCALE * TAU + time * INTERFERENCE_SPEED);
        let w2 = sin(dot(local, vec2<f32>(-0.4, 1.0)) * INTERFERENCE_SCALE * TAU - time * INTERFERENCE_SPEED * 0.8);
        bright = 0.55 + 0.45 * (w1 + w2) * 0.5; // ~0.1 .. 1.0
    }

    // Combine the two warped grids into the moiré lattice.
    var lattice = 0.0;
    if ENABLE_MOIRE {
        let warp = dwd_value_noise_curl_2d(local * MOIRE_WARP_SCALE + vec2<f32>(time * MOIRE_WARP_SPEED + seed, 0.0)) * MOIRE_WARP_STRENGTH;
        let la = grid_wave(local * MOIRE_SCALE_A + warp + vec2<f32>(time * 0.05, 0.0));
        let lb = grid_wave(rotate(local, MOIRE_ROT) * MOIRE_SCALE_B + warp);
        lattice = la * lb;
    }

    // Add cyan-green accents to the brightest violet lattice nodes.
    let lit = lattice * bright;
    var overlay = COLOR_BASE   * lit * MOIRE_INTENSITY
                + COLOR_ACCENT * pow(clamp(lit, 0.0, 1.0), 4.0) * ACCENT_AMOUNT;
    // Scan-collapse eases the chaotic lattice toward calm where the field is being observed.
    overlay *= masks.interior * masks.weirdness * (1.0 - collapse);

    // ── Boundary ───────────────────────────────────────────────────────────────
    var boundary_col = vec3<f32>(0.0);
    if ENABLE_BOUNDARY {
        boundary_col += COLOR_BOUNDARY * masks.on_edge * (0.6 + 0.4 * masks.weirdness);
    }

    // Faint interior tint so the area still reads when calm/solved (also eased by collapse).
    let base_tint = COLOR_BASE * masks.interior * (0.05 + 0.10 * masks.weirdness) * (1.0 - collapse);

    return overlay + boundary_col + base_tint;
}
