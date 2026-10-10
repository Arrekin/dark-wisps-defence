import bevy_sprite_render::mesh2d::vertex_output::VertexOutput;
import bevy_sprite_render::mesh2d::view_bindings::globals;
import package::shaders::wisps::fire_look::{WispFireLook, wisp_fire};

// Map-material bindings for the shared fire-wisp shader.

@group(2) @binding(4)
var<uniform> uniforms: WispFireLook;

const WISP_EFFECT_SLOTS: u32 = 8u;
struct WispEffects {
    mask: u32,
    params: array<vec4<f32>, WISP_EFFECT_SLOTS>,
};
@group(2) @binding(5)
var<uniform> effects: WispEffects;

struct WispQuad {
    alpha: f32,
};
@group(2) @binding(6)
var<uniform> quad: WispQuad;

@fragment
fn fragment(mesh: VertexOutput) -> @location(0) vec4<f32> {
    var color = wisp_fire(mesh.uv, globals.time, uniforms, effects.mask);
    color.a *= quad.alpha;
    return color;
}
