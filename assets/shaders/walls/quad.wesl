import bevy_sprite_render::mesh2d::vertex_output::VertexOutput;
import package::shaders::common::core::CELL_SIZE;
import package::shaders::walls::look::single_cell_wall;
import package::shaders::walls::styles::wall_style;

// One isolated wall cell in world space, for placement previews.

// Field order and types mirror `WallQuadMaterial` in map_objects_internal/src/wall_materials.rs.
struct WallQuad {
    style_index: u32, // one-based index into the shared wall style table
    alpha: f32,
}
@group(2) @binding(0) var<uniform> settings: WallQuad;

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    // Mesh UV has y running down and the canvas has it running up. Flipping here keeps the
    // light on the same side of the wall as it is on the map.
    let world = vec2<f32>(in.uv.x, 1.0 - in.uv.y) * CELL_SIZE;

    // World-coordinate span of one screen pixel at the quad's rendered size.
    let texel = max(fwidth(world.x), 0.001);
    let wall = single_cell_wall(world, texel, wall_style(settings.style_index));
    return vec4<f32>(wall.rgb, wall.a * settings.alpha);
}
