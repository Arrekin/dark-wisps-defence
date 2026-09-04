#import bevy_render::globals::Globals
#import bevy_ui::ui_vertex_output::UiVertexOutput
#import dwd::quantum_field::{dwd_quantum_field_masks, dwd_quantum_field_glow}

// Quantum-field UI face. It renders the shared procedural glow without the map-only frame
// distortion. Bind group 0 provides global time; the material has no custom bindings.

@group(0) @binding(1)
var<uniform> globals: Globals;

// Tile faces use a fixed seed and show an unsolved field.
const FACE_SEED: f32 = 0.0;
const FACE_SOLVE_PROGRESS: f32 = 0.0;

// Samples three 32px grid cells so several lattice cells remain visible at tile size.
const FACE_SPAN: f32 = 96.0;
// Prevents boundary jitter from clipping at the node edge.
const FACE_INSET: f32 = 10.0;

@fragment
fn fragment(in: UiVertexOutput) -> @location(0) vec4<f32> {
    let local = (in.uv - vec2<f32>(0.5)) * FACE_SPAN;
    let half_extent = vec2<f32>(FACE_SPAN * 0.5 - FACE_INSET);

    let masks = dwd_quantum_field_masks(local, half_extent, local, globals.time, FACE_SEED, FACE_SOLVE_PROGRESS);
    let glow = dwd_quantum_field_glow(local, masks, globals.time, FACE_SEED, 0.0);

    // Use emitted brightness as alpha so empty space between glow features remains transparent.
    let alpha = clamp(max(glow.r, max(glow.g, glow.b)), 0.0, 1.0);
    return vec4<f32>(glow, alpha);
}
