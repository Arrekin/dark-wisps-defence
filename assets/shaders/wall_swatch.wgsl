#import bevy_ui::ui_vertex_output::UiVertexOutput
#import dwd::core::CELL_SIZE
#import dwd::map_light::MAP_SUN_GROUND_DIRECTION
#import dwd::wall_style::{WallStyle, LIGHT_PROBE, eroded_distance, plate_noise, wall_shading}

// Renders one isolated wall cell for style selection. An analytic box distance replaces the map
// canvas's neighbor-derived field; all surface layers still come from `dwd::wall_style`.
// Contact shadow is omitted because the swatch is composited onto a UI panel.

@group(1) @binding(0) var<uniform> style: WallStyle;

// Signed distance in world pixels, positive inside the tile.
fn distance_at(world: vec2<f32>) -> f32 {
    let from_centre = abs(world - vec2<f32>(CELL_SIZE * 0.5));
    return CELL_SIZE * 0.5 - max(from_centre.x, from_centre.y);
}

@fragment
fn fragment(in: UiVertexOutput) -> @location(0) vec4<f32> {
    // UI space has y running down and the canvas has it running up. Flipping here keeps the
    // light on the same side of the wall in the swatch as it is on the map.
    let world = vec2<f32>(in.uv.x, 1.0 - in.uv.y) * CELL_SIZE;

    // World-coordinate span of one screen pixel at the UI node's rendered size.
    let texel = CELL_SIZE / max(in.size.x, 1.0);

    let raw_distance = distance_at(world);
    let d = eroded_distance(raw_distance, world, style);

    let probe = max(LIGHT_PROBE, texel);
    let lit = -(distance_at(world + MAP_SUN_GROUND_DIRECTION * probe) - raw_distance) / probe;

    let plate = plate_noise(world, style);
    let colour = wall_shading(d, lit, plate, texel, style);

    return vec4<f32>(colour, smoothstep(-texel, texel, d));
}
