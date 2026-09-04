#import bevy_render::globals::Globals
#import bevy_ui::ui_vertex_output::UiVertexOutput
#import dwd::wisps::electric::{WispElectricLook, dwd_wisp_electric}

// Stationary electric-wisp UI face with alteration effects disabled.

@group(0) @binding(1)
var<uniform> globals: Globals;

const FACE_SEED: f32 = 0.0;

@fragment
fn fragment(in: UiVertexOutput) -> @location(0) vec4<f32> {
    var look: WispElectricLook;
    look.seed = FACE_SEED;
    look.vigor = 0.0;
    look.heading_x = 0.0;
    look.heading_y = 0.0;
    return dwd_wisp_electric(in.uv, globals.time, look, 0u);
}
