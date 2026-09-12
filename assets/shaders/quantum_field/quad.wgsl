#import bevy_sprite::mesh2d_vertex_output::VertexOutput
#import bevy_sprite::mesh2d_view_bindings::globals
#import dwd::quantum_field::{dwd_quantum_field_masks, dwd_quantum_field_glow}

// One field in world space, spanning `quad_size` world pixels. It renders the shared procedural
// glow without the map-only frame distortion.

struct QuantumFieldQuad {
    quad_size: vec2<f32>,
    alpha: f32,
}
@group(2) @binding(0) var<uniform> settings: QuantumFieldQuad;

const SEED: f32 = 0.0;
const SOLVE_PROGRESS: f32 = 0.0;
// Prevents boundary jitter from clipping at the quad edge.
const INSET: f32 = 10.0;

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    let local = (in.uv - vec2<f32>(0.5)) * settings.quad_size;
    let half_extent = settings.quad_size * 0.5 - vec2<f32>(INSET);

    let masks = dwd_quantum_field_masks(local, half_extent, local, globals.time, SEED, SOLVE_PROGRESS);
    let glow = dwd_quantum_field_glow(local, masks, globals.time, SEED, 0.0);

    // Emitted brightness becomes opacity, so empty space between glow features stays transparent.
    let emitted = clamp(max(glow.r, max(glow.g, glow.b)), 0.0, 1.0);
    return vec4<f32>(glow, emitted * settings.alpha);
}
