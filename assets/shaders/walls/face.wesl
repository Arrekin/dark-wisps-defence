import bevy_ui_render::ui_vertex_output::UiVertexOutput;
import package::shaders::common::core::CELL_SIZE;
import package::shaders::walls::look::single_cell_wall;
import package::shaders::walls::styles::wall_style;

// One isolated wall cell as a UI swatch for style selection.

// One-based index into the shared wall style table.
@group(1) @binding(0) var<uniform> style_index: u32;

@fragment
fn fragment(in: UiVertexOutput) -> @location(0) vec4<f32> {
    // UI space has y running down and the canvas has it running up. Flipping here keeps the
    // light on the same side of the wall in the swatch as it is on the map.
    let world = vec2<f32>(in.uv.x, 1.0 - in.uv.y) * CELL_SIZE;

    // World-coordinate span of one screen pixel at the UI node's rendered size.
    let texel = CELL_SIZE / max(in.size.x, 1.0);

    return single_cell_wall(world, texel, wall_style(style_index));
}
