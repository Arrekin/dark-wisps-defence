// Quantum field overlay that distorts and duplicates the scene beneath the field's rectangle.
// Samples the ground and surface bands before the air band draws.

import bevy_sprite_render::mesh2d::{vertex_output::VertexOutput, view_bindings::globals};
import package::shaders::common::beneath::beneath_at_world;
import package::shaders::common::core::TAU;
import package::shaders::common::value_noise::value_fbm_2d;
import package::shaders::quantum_field::look::{quantum_field_masks, quantum_field_glow, box_sdf, COLOR_BOUNDARY};

// Mirrors `MAX_COLLAPSE_POINTS` in map_objects_internal/src/quantum_field_anomaly.rs.
const MAX_COLLAPSE_POINTS: u32 = 4u;

// Field order and types mirror `QuantumFieldAnomaly` in map_objects_internal/src/quantum_field_anomaly.rs.
struct Instance {
    center:         vec2<f32>,
    half_extent:    vec2<f32>,
    solve_progress: f32, // 0 = fresh anomaly, 1 = solved
    seed:           f32, // per-field noise offset
    collapse_count: u32,
    // Ground positions of the scan-beam spots observing this field.
    collapse_points: array<vec2<f32>, MAX_COLLAPSE_POINTS>,
};
// Fields gathered for this frame, used to assign each pixel to one field.
// `READS_ALL_INSTANCES` makes the generated entry shader import this declaration.
@group(2) @binding(0) var<storage, read> instances: array<Instance>;

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

fn overlay(in: VertexOutput, field: Instance) -> vec4<f32> {
    let world_pos = in.world_position.xy;
    let t = globals.time;

    // Fields never overlap (placement validator), but their quads' margins can; each pixel
    // belongs to the deepest box (smallest SDF), and other quads leave it alone.
    var best_sdf = 1.0e9;
    var best_idx = 0u;
    for (var i = 0u; i < arrayLength(&instances); i++) {
        let sdf = box_sdf(world_pos - instances[i].center, instances[i].half_extent);
        if sdf < best_sdf {
            best_sdf = sdf;
            best_idx = i;
        }
    }
    if best_idx != in.instance_index {
        discard;
    }

    let to_center = world_pos - field.center;
    let masks = quantum_field_masks(to_center, field.half_extent, world_pos, t, field.seed, field.solve_progress);

    // Anything well outside the (possibly wobbled) edge is untouched.
    if masks.edge_sdf > BOUNDARY_OUTER_BAND {
        discard;
    }

    // ── Scan-collapse: where a drone beam observes the field, the anomaly calms locally ──
    // For each active scan spot, calm a disc around it: superposition + decoherence snap toward
    // the real object and the chaotic lattice eases (observation collapses the wavefunction).
    var collapse = 0.0;
    if ENABLE_SCAN_COLLAPSE {
        for (var i = 0u; i < field.collapse_count; i++) {
            let d = distance(world_pos, field.collapse_points[i]);
            collapse = max(collapse, smoothstep(SCAN_COLLAPSE_RADIUS, SCAN_COLLAPSE_RADIUS * 0.35, d));
        }
        collapse *= masks.interior;
    }

    // ── Build the world-space coordinate we sample the scene from ──────────────
    var sample_world = world_pos;

    if ENABLE_SCHLIEREN {
        let nx = value_fbm_2d(world_pos * SCHLIEREN_SCALE + vec2<f32>(t * 0.20 + field.seed, 0.0));
        let ny = value_fbm_2d(world_pos * SCHLIEREN_SCALE + vec2<f32>(0.0, t * 0.17 + field.seed));
        sample_world += (vec2<f32>(nx, ny) - 0.5) * 2.0 * SCHLIEREN_STRENGTH * masks.weirdness * masks.interior;
    }
    // Inside a collapse disc, pin sampling back to the true position — schlieren and any other
    // displacement vanish, so the observed area shows crystal-clear reality (rim still distorts).
    sample_world = mix(sample_world, world_pos, collapse);

    // ── Sample the scene: superposition ghosting + chromatic decoherence ───────
    var rgb: vec3<f32>;
    if ENABLE_SUPERPOSITION {
        let spread = SUPERPOSITION_OFFSET * masks.weirdness * masks.interior * (1.0 - collapse);
        var acc  = beneath_at_world(sample_world).rgb * 2.0; // real position weighted higher
        var wsum = 2.0;
        for (var k = 0; k < SUPERPOSITION_TAPS; k++) {
            let ang = TAU * f32(k) / f32(SUPERPOSITION_TAPS) + t * SUPERPOSITION_SPIN;
            let off = vec2<f32>(cos(ang), sin(ang)) * spread;
            acc += beneath_at_world(sample_world + off).rgb;
            wsum += 1.0;
        }
        rgb = acc / wsum;
    } else {
        rgb = beneath_at_world(sample_world).rgb;
    }
    if ENABLE_DECOHERENCE {
        let split = DECOHERENCE_SPLIT * masks.weirdness * masks.interior * (1.0 - collapse);
        let len   = length(to_center);
        let dir   = select(vec2<f32>(1.0, 0.0), to_center / max(len, 0.001), len > 0.001);
        let cr = beneath_at_world(sample_world + dir * split).r;
        let cb = beneath_at_world(sample_world - dir * split).b;
        rgb = vec3<f32>(cr, rgb.g, cb);
    }

    // ── Phase flicker near the rim (screen-space approximation of tunneling) ───
    if ENABLE_PHASE_FLICKER {
        let f = value_fbm_2d(world_pos * 0.2 + vec2<f32>(t * PHASE_FLICKER_SPEED, field.seed));
        rgb *= 1.0 - PHASE_FLICKER_DEPTH * masks.rim_zone * masks.weirdness * step(0.5, f) * (1.0 - collapse);
    }

    var result = rgb + quantum_field_glow(to_center, masks, t, field.seed, collapse);

    // Scan-collapse: soft violet rim marking each observed/calmed disc (0 when nothing scanning).
    let collapse_rim = collapse * (1.0 - collapse) * 4.0; // peaks at the disc edge
    result += COLOR_BOUNDARY * collapse_rim * SCAN_GLOW;

    return vec4<f32>(result, beneath_at_world(world_pos).a);
}
