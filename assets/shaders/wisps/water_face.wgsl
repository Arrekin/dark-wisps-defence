#import bevy_render::globals::Globals
#import bevy_ui::ui_vertex_output::UiVertexOutput
#import dwd::wisps::water::{WispWaterLook, dwd_wisp_water}

// Stationary water-wisp UI face with alteration effects disabled.

@group(0) @binding(1)
var<uniform> globals: Globals;

const FACE_SEED: f32 = 0.0;

@fragment
fn fragment(in: UiVertexOutput) -> @location(0) vec4<f32> {
    var look: WispWaterLook;
    look.seed = FACE_SEED;
    look.wobble = 0.06;
    look.flow_speed = 1.0;
    look.tint = 0.0;
    look.heading_x = 0.0;
    look.heading_y = 0.0;
    look.vigor = 0.0;
    look.stroke_anchor_phase = 0.0;
    look.surf_anchor_phase = 0.0;
    look.anchor_time = 0.0;
    return dwd_wisp_water(in.uv, globals.time, look, 0u);
}
