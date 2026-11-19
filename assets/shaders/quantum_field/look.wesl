// Procedural quantum field glow: boundary, interior tint and moiré lattice.

import package::shaders::common::clock::wrap_rate;
import package::shaders::common::core::TAU;
import package::shaders::common::noise::{noise_curl_2d_drift, noise_fbm_2d_drift};

// ── Palette ───────────────────────────────────────────────────────────────────
const COLOR_BASE:     vec3<f32> = vec3<f32>(0.45, 0.20, 0.85); // violet interior
const COLOR_ACCENT:   vec3<f32> = vec3<f32>(0.25, 1.00, 0.70); // cyan-green spark on bright lattice nodes
const COLOR_BOUNDARY: vec3<f32> = vec3<f32>(0.60, 0.45, 1.00); // brighter violet rim — matches the interior

const BOUNDARY_THICKNESS:             f32 = 2.5;   // visible rim line half-width
const INTERIOR_FADE:                  f32 = 6.0;   // distance the interior ramps to full strength over;
                                                   // also the object-effect edge falloff distance
const BOUNDARY_UNCERTAINTY_AMPLITUDE: f32 = 5.0;   // edge jitter amplitude
const BOUNDARY_UNCERTAINTY_FREQUENCY: f32 = 0.05;  // edge jitter spatial frequency

// ── Interior pattern tunables ─────────────────────────────────────────────────
// Moiré lattice from two differently scaled grids warped by curl noise; the interior's visible structure.
const MOIRE_SCALE_A:       f32 = 0.10;
const MOIRE_SCALE_B:       f32 = 0.115;  // slightly different scale → beat pattern
const MOIRE_ROTATION:      f32 = 0.20;   // radians between the two lattices
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

// Spatial and state masks for a quantum field sample.
struct QuantumFieldMasks {
    // Signed distance to the jittered boundary in world pixels (negative inside).
    edge_sdf: f32,
    // 1 for an unsolved anomaly, 0 when solved.
    weirdness: f32,
    // 1 deep inside, 0 at the boundary.
    interior: f32,
    // Band straddling the boundary (`INTERIOR_FADE` wide).
    rim_zone: f32,
    // Visible boundary stroke (`BOUNDARY_THICKNESS` wide).
    on_edge: f32,
}

// Computes boundary and interior masks at `local` (offset from the field centre, in world pixels).
// `noise_at` is the world-space sample position for boundary jitter so adjacent fields do not
// wobble in phase. `time` is the wrapped shader clock (`globals.time`).
fn quantum_field_masks(
    local: vec2<f32>,
    half_extent: vec2<f32>,
    noise_at: vec2<f32>,
    time: f32,
    seed: f32,
    solve_progress: f32,
) -> QuantumFieldMasks {
    // Uncertain boundary: wobble the effective edge used for masking.
    let jitter = noise_fbm_2d_drift(noise_at * BOUNDARY_UNCERTAINTY_FREQUENCY + vec2<f32>(seed, 0.0), time, vec2<f32>(0.0, 0.3), 2) - 0.5;
    let edge_sdf = box_sdf(local, half_extent) + jitter * 2.0 * BOUNDARY_UNCERTAINTY_AMPLITUDE;

    let weirdness = 1.0 - solve_progress;
    // Interior mask: 1 inside, fading to 0 at the edge.
    let interior  = 1.0 - smoothstep(-INTERIOR_FADE, 0.0, edge_sdf);
    // Edge band used by phase flicker.
    let rim_zone  = 1.0 - smoothstep(0.0, INTERIOR_FADE, abs(edge_sdf));
    // Boundary-line mask.
    let on_edge   = 1.0 - smoothstep(0.0, BOUNDARY_THICKNESS, abs(edge_sdf));

    return QuantumFieldMasks(edge_sdf, weirdness, interior, rim_zone, on_edge);
}

// Additive field glow combining the moiré lattice, boundary line, and interior tint.
// `collapse` (`0..1`, raised by drone scan beams) suppresses the lattice and tint.
fn quantum_field_glow(
    local: vec2<f32>,
    masks: QuantumFieldMasks,
    time: f32,
    seed: f32,
    collapse: f32,
) -> vec3<f32> {
    // Drifting standing-wave brightness modulation.
    let wave_a = sin(dot(local, vec2<f32>(1.0, 0.3))  * INTERFERENCE_SCALE * TAU + time * wrap_rate(INTERFERENCE_SPEED, TAU));
    let wave_b = sin(dot(local, vec2<f32>(-0.4, 1.0)) * INTERFERENCE_SCALE * TAU - time * wrap_rate(INTERFERENCE_SPEED * 0.8, TAU));
    let bright = 0.55 + 0.45 * (wave_a + wave_b) * 0.5; // ~0.1 .. 1.0

    // Combine the two warped grids into the moiré lattice.
    let warp = noise_curl_2d_drift(local * MOIRE_WARP_SCALE + vec2<f32>(seed, 0.0), time, vec2<f32>(MOIRE_WARP_SPEED, 0.0)) * MOIRE_WARP_STRENGTH;
    let lattice_a = grid_wave(local * MOIRE_SCALE_A + warp + vec2<f32>(time * wrap_rate(0.05, 1.0), 0.0));
    let lattice_b = grid_wave(rotate(local, MOIRE_ROTATION) * MOIRE_SCALE_B + warp);
    let lattice = lattice_a * lattice_b;

    // Add cyan-green accents to the brightest violet lattice nodes.
    let lit = lattice * bright;
    var overlay = COLOR_BASE   * lit * MOIRE_INTENSITY
                + COLOR_ACCENT * pow(clamp(lit, 0.0, 1.0), 4.0) * ACCENT_AMOUNT;
    // Scan-collapse eases the chaotic lattice toward calm where the field is being observed.
    overlay *= masks.interior * masks.weirdness * (1.0 - collapse);

    let boundary_color = COLOR_BOUNDARY * masks.on_edge * (0.6 + 0.4 * masks.weirdness);

    // Faint interior tint so the area still reads when calm/solved (also eased by collapse).
    let base_tint = COLOR_BASE * masks.interior * (0.05 + 0.10 * masks.weirdness) * (1.0 - collapse);

    return overlay + boundary_color + base_tint;
}
