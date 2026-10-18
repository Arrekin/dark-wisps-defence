import bevy_sprite_render::mesh2d::vertex_output::VertexOutput;
import bevy_sprite_render::mesh2d::view_bindings::globals;
import package::shaders::common::effect_visuals::EffectVisuals;
import package::shaders::wisps::electric_look::wisp_electric;
import package::shaders::wisps::motion_look::WispMotionLook;

// Lane adapter for the electric wisp: the paint lane binds one `Instance` per wisp.

// Field order and types mirror `WispElectricVisual` in wisps_internal/src/looks.rs.
struct Instance {
    look: WispMotionLook,
    effects: EffectVisuals,
    alpha: f32, // below 1.0 for a placement ghost
};

fn look(mesh: VertexOutput, instance: Instance) -> vec4<f32> {
    var color = wisp_electric(mesh.uv, globals.time, instance.look, instance.effects.mask);
    color.a *= instance.alpha;
    return color;
}
