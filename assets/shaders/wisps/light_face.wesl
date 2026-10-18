import bevy_render::globals::Globals;
import bevy_ui_render::ui_vertex_output::UiVertexOutput;
import package::shaders::wisps::light_look::wisp_light;
import package::shaders::wisps::motion_look::WispMotionLook;

// Stationary light-wisp UI face with alteration effects disabled.

@group(0) @binding(1)
var<uniform> globals: Globals;

const FACE_SEED: f32 = 0.0;

@fragment
fn fragment(in: UiVertexOutput) -> @location(0) vec4<f32> {
    let look = WispMotionLook(FACE_SEED, 0.0, vec2<f32>(0.0));
    return wisp_light(in.uv, globals.time, look, 0u);
}
