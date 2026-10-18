import bevy_sprite_render::mesh2d::vertex_output::VertexOutput;
import bevy_sprite_render::mesh2d::view_bindings::globals;
import package::shaders::common::effect_visuals::EffectVisuals;
import package::shaders::wisps::water_look::{WispWaterLook, wisp_water};

// Lane adapter for the water wisp: the paint lane binds one `Instance` per wisp.

// Field order and types mirror `WispWaterVisual` in wisps_internal/src/looks.rs.
struct Instance {
    look: WispWaterLook,
    effects: EffectVisuals,
    alpha: f32, // below 1.0 for a placement ghost
};

fn look(mesh: VertexOutput, instance: Instance) -> vec4<f32> {
    var color = wisp_water(mesh.uv, globals.time, instance.look, instance.effects.mask);
    color.a *= instance.alpha;
    return color;
}
