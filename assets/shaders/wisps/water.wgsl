#import bevy_sprite::mesh2d_vertex_output::VertexOutput
#import bevy_sprite::mesh2d_view_bindings::globals
#import dwd::wisps::water::{WispWaterLook, dwd_wisp_water}

// Map-material bindings for the shared water-wisp shader.

@group(2) @binding(4)
var<uniform> uniforms: WispWaterLook;

const WISP_EFFECT_SLOTS: u32 = 8u;
struct WispEffects {
    mask: u32,
    params: array<vec4<f32>, WISP_EFFECT_SLOTS>,
};
@group(2) @binding(5)
var<uniform> effects: WispEffects;

@fragment
fn fragment(mesh: VertexOutput) -> @location(0) vec4<f32> {
    return dwd_wisp_water(mesh.uv, globals.time, uniforms, effects.mask);
}
