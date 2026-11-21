import bevy_render::globals::Globals;
import bevy_ui_render::ui_vertex_output::UiVertexOutput;
import package::shaders::common::clock::ClockInstant;
import package::shaders::grids::obstacles::LEVEL_NONE;
import package::shaders::wisps::water_look::{WispWaterLook, wisp_water};

// Stationary water-wisp UI face with alteration effects disabled.

@group(0) @binding(1)
var<uniform> globals: Globals;

// Spawn instant on Bevy's shader clock. Mirrors `WispWaterFaceMaterial` in wisps_internal/src/face.rs.
@group(1) @binding(0)
var<uniform> anchor: ClockInstant;

const FACE_SEED: f32 = 0.0;

@fragment
fn fragment(in: UiVertexOutput) -> @location(0) vec4<f32> {
    var look: WispWaterLook;
    look.seed = FACE_SEED;
    look.wobble = 0.06;
    look.flow_speed = 1.0;
    look.tint = 0.0;
    look.heading = vec2<f32>(0.0);
    look.vigor = 0.0;
    look.stroke_anchor_phase = 0.0;
    look.surf_anchor_phase = 0.0;
    look.anchor = anchor;
    return wisp_water(in.uv, globals.time, look, 0u, vec2<f32>(0.0), LEVEL_NONE);
}
