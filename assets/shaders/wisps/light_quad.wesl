import bevy_sprite_render::mesh2d::vertex_output::VertexOutput;
import bevy_sprite_render::mesh2d::view_bindings::globals;
import package::shaders::common::effect_visuals::EffectVisuals;
import package::shaders::wisps::light_look::wisp_light;
import package::shaders::wisps::motion_look::WispMotionLook;

// Lane adapter for the light wisp: the paint lane binds one `Instance` per wisp.

// Field order and types mirror `WispLightVisual` in wisps_internal/src/looks.rs.
struct Instance {
    look: WispMotionLook,
    effects: EffectVisuals,
    alpha: f32, // below 1.0 for a placement ghost
};

fn look(mesh: VertexOutput, instance: Instance) -> vec4<f32> {
    var color = wisp_light(mesh.uv, globals.time, instance.look, instance.effects.mask);
    color.a *= instance.alpha;
    return color;
}
