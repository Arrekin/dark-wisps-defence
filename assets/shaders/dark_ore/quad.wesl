import bevy_sprite_render::mesh2d::vertex_output::VertexOutput;
import package::shaders::common::core::CELL_SIZE;
import package::shaders::dark_ore::look::{dark_ore_shading, ore_inset, single_cell_distance};

// Draws one full dark-ore deposit as a world-space placement preview.

struct DarkOreQuad {
    alpha: f32,
}
@group(2) @binding(0) var<uniform> settings: DarkOreQuad;

// A deposit is placed at the almanach's max field saturation, so one drawn on its own is full.
const FRESH_DEPOSIT_FILL: f32 = 1.0;

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    // Mesh UV has y running down and the canvas has it running up. Flipping here keeps the light
    // on the same side of the crystals as it is on the map.
    let p = vec2<f32>(in.uv.x, 1.0 - in.uv.y) - vec2<f32>(0.5);
    let world = (p + vec2<f32>(0.5)) * CELL_SIZE;

    // World-coordinate span of one screen pixel at the quad's rendered size.
    let texel = max(max(fwidth(world.x), fwidth(world.y)), 0.0001);

    let d = single_cell_distance(p) * CELL_SIZE - ore_inset(world);
    if d <= 0.0 { return vec4<f32>(0.0); }

    let shaded = dark_ore_shading(d, FRESH_DEPOSIT_FILL, texel, world);
    return vec4<f32>(shaded.rgb, shaded.a * settings.alpha);
}
