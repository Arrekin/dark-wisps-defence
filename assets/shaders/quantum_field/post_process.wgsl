#import bevy_core_pipeline::fullscreen_vertex_shader::FullscreenVertexOutput
#import dwd::core::TAU
#import dwd::value_noise::dwd_value_fbm_2d
#import dwd::quantum_field::{QuantumFieldMasks, dwd_quantum_field_masks, dwd_quantum_field_glow, box_sdf, COLOR_BOUNDARY}

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

// ── Effect switches ──────────────────────────────────────────────────────────
const ENABLE_SCHLIEREN:            bool = true;
const ENABLE_SUPERPOSITION:        bool = true;
const ENABLE_DECOHERENCE:          bool = true;
const ENABLE_PHASE_FLICKER:        bool = true;
const ENABLE_SCAN_COLLAPSE:        bool = true;  // drone beams locally calm the field around their scan spot

// ── Boundary tunables (world units unless noted) ──────────────────────────────
const BOUNDARY_OUTER_BAND:  f32 = 8.0;   // distance past the edge still processed

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

    let to_center = world_pos - field.center;
    let masks = dwd_quantum_field_masks(to_center, field.half_extent, world_pos, t, field.seed, field.solve_progress);

    // Anything well outside the (possibly wobbled) edge is untouched.
    if masks.edge_sdf > BOUNDARY_OUTER_BAND {
        return textureSampleLevel(screen_texture, screen_sampler, in.uv, 0.0);
    }

    // ── Scan-collapse: where a drone beam observes the field, the anomaly calms locally ──
    // For each active scan spot, calm a disc around it: superposition + decoherence snap toward
    // the real object and the chaotic lattice eases (observation collapses the wavefunction).
    var collapse = 0.0;
    if ENABLE_SCAN_COLLAPSE {
        for (var i = 0u; i < camera.collapse_count; i++) {
            let d = distance(world_pos, collapse_points[i].pos);
            collapse = max(collapse, smoothstep(SCAN_COLLAPSE_RADIUS, SCAN_COLLAPSE_RADIUS * 0.35, d));
        }
        collapse *= masks.interior;
    }

    // ── Build the world-space coordinate we sample the frame from ──────────────
    var sample_world = world_pos;

    if ENABLE_SCHLIEREN {
        let nx = dwd_value_fbm_2d(world_pos * SCHLIEREN_SCALE + vec2<f32>(t * 0.20 + field.seed, 0.0));
        let ny = dwd_value_fbm_2d(world_pos * SCHLIEREN_SCALE + vec2<f32>(0.0, t * 0.17 + field.seed));
        sample_world += (vec2<f32>(nx, ny) - 0.5) * 2.0 * SCHLIEREN_STRENGTH * masks.weirdness * masks.interior;
    }
    // Inside a collapse disc, pin sampling back to the true position — schlieren and any other
    // displacement vanish, so the observed area shows crystal-clear reality (rim still distorts).
    sample_world = mix(sample_world, world_pos, collapse);

    // ── Sample the frame: superposition ghosting + chromatic decoherence ───────
    var rgb: vec3<f32>;
    if ENABLE_SUPERPOSITION {
        let spread = SUPERPOSITION_OFFSET * masks.weirdness * masks.interior * (1.0 - collapse);
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
        let split = DECOHERENCE_SPLIT * masks.weirdness * masks.interior * (1.0 - collapse);
        let len   = length(to_center);
        let dir   = select(vec2<f32>(1.0, 0.0), to_center / max(len, 0.001), len > 0.001);
        let cr = sample_rgb(world_to_uv(sample_world + dir * split)).r;
        let cb = sample_rgb(world_to_uv(sample_world - dir * split)).b;
        rgb = vec3<f32>(cr, rgb.g, cb);
    }

    // ── Phase flicker near the rim (screen-space approximation of tunneling) ───
    if ENABLE_PHASE_FLICKER {
        let f = dwd_value_fbm_2d(world_pos * 0.2 + vec2<f32>(t * PHASE_FLICKER_SPEED, field.seed));
        rgb *= 1.0 - PHASE_FLICKER_DEPTH * masks.rim_zone * masks.weirdness * step(0.5, f) * (1.0 - collapse);
    }

    var result = rgb + dwd_quantum_field_glow(to_center, masks, t, field.seed, collapse);

    // Scan-collapse: soft violet rim marking each observed/calmed disc (0 when nothing scanning).
    let collapse_rim = collapse * (1.0 - collapse) * 4.0; // peaks at the disc edge
    result += COLOR_BOUNDARY * collapse_rim * SCAN_GLOW;

    return vec4<f32>(result, 1.0);
}
