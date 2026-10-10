import bevy_ui_render::ui_vertex_output::UiVertexOutput;
import package::shaders::common::core::CELL_SIZE;
import package::shaders::dark_ore::look::{dark_ore_shading, ore_inset, single_cell_distance};

// Draws one full dark-ore deposit in a UI node.

// A deposit is placed at the almanach's max field saturation, so one drawn on its own is full.
const FRESH_DEPOSIT_FILL: f32 = 1.0;

@fragment
fn fragment(in: UiVertexOutput) -> @location(0) vec4<f32> {
    // UI space has y running down and the canvas has it running up. Flipping here keeps the light
    // on the same side of the crystals in the tile as it is on the map.
    let p = vec2<f32>(in.uv.x, 1.0 - in.uv.y) - vec2<f32>(0.5);
    let world = (p + vec2<f32>(0.5)) * CELL_SIZE;

    // World-coordinate span of one screen pixel at the UI node's rendered size.
    let texel = CELL_SIZE / max(in.size.x, 1.0);

    let d = single_cell_distance(p) * CELL_SIZE - ore_inset(world);
    if d <= 0.0 { return vec4<f32>(0.0); }

    return dark_ore_shading(d, FRESH_DEPOSIT_FILL, texel, world);
}
