import bevy_sprite_render::mesh2d::vertex_output::VertexOutput;
import package::shaders::visuals::drop_shadow_cast::{cast_alpha, surface_rise};

// Circular drop shadow projected onto cell top surfaces via `drop_shadow_cast`.
// Evaluated in world space; the mesh quad only needs to cover the shadow's bounds.

// Field order and types mirror `DropShadow` in visuals/src/drop_shadow.rs.
struct Instance {
    ground: vec2<f32>,  // world position directly below the caster
    base: vec2<f32>,    // world position of the reference cell for `height`
    height: f32,        // caster height above the base cell's top surface, in world units
    diameter: f32,      // shadow diameter at zero height, in world units
    opacity: f32,       // opacity multiplier
};

fn look(mesh: VertexOutput, shadow: Instance) -> vec4<f32> {
    let p = mesh.world_position.xy;
    let rise = surface_rise(p, shadow.base);
    return vec4<f32>(0.0, 0.0, 0.0, shadow.opacity * cast_alpha(p, shadow.ground, rise, shadow.height, shadow.diameter));
}
