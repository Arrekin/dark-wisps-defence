import bevy_sprite_render::mesh2d::vertex_output::VertexOutput;
import bevy_sprite_render::mesh2d::view_bindings::globals;
import package::shaders::common::core::CELL_SIZE;
import package::shaders::common::effect_visuals::EffectVisuals;
import package::shaders::wisps::fire_look::{QUAD_SCALE, wisp_fire};
import package::shaders::wisps::motion_look::WispMotionLook;

// Lane adapter for the fire wisp: the paint lane binds one `Instance` per wisp.

// Field order and types mirror `WispFireVisual` in wisps_internal/src/looks.rs.
struct Instance {
    look: WispMotionLook,
    effects: EffectVisuals,
    alpha: f32, // below 1.0 for a placement ghost
    altitude_index: u32, // LEVEL_* code in grids/obstacles.wesl of the level the wisp is on
};

fn look(mesh: VertexOutput, instance: Instance) -> vec4<f32> {
    // The quad is unrotated and centred on the wisp; UV v runs down while world y runs up.
    let centre = mesh.world_position.xy - (mesh.uv - vec2<f32>(0.5)) * vec2<f32>(1.0, -1.0) * QUAD_SCALE * CELL_SIZE;
    var color = wisp_fire(mesh.uv, globals.time, instance.look, instance.effects.mask, centre, instance.altitude_index);
    color.a *= instance.alpha;
    return color;
}
