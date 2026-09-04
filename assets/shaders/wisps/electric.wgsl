#import bevy_sprite::mesh2d_vertex_output::VertexOutput
#import bevy_sprite::mesh2d_view_bindings::globals
#import dwd::wisps::electric::{WispElectricLook, dwd_wisp_electric}

// Map-material bindings for the shared electric-wisp shader.

@group(2) @binding(4)
var<uniform> uniforms: WispElectricLook;

const WISP_EFFECT_SLOTS: u32 = 8u;
struct WispEffects {
    mask: u32,
    params: array<vec4<f32>, WISP_EFFECT_SLOTS>,
};
@group(2) @binding(5)
var<uniform> effects: WispEffects;

@fragment
fn fragment(mesh: VertexOutput) -> @location(0) vec4<f32> {
    return dwd_wisp_electric(mesh.uv, globals.time, uniforms, effects.mask);
}
