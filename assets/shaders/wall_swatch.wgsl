#import bevy_ui::ui_vertex_output::UiVertexOutput
#import dwd::wall_style::{WallStyle, LIGHT_PROBE, eroded_distance, plate_noise, wall_shading}

// Wall swatch
//
// One wall cell drawn on its own, so a style can be picked by looking at it. The map canvas
// builds its distance field from a cell's eight neighbours; a lone tile has none, so the field
// here is the analytic form of the same shape - the distance to the nearest of the four edges.
// Every layer comes from `dwd::wall_style`, so a swatch cannot drift from what the wall will
// look like once it is placed.
//
// The contact shadow is the one thing the map draws that this does not, because a swatch sits
// on a panel rather than on open ground.

@group(1) @binding(0) var<uniform> style: WallStyle;

const CELL_SIZE: f32 = 32.0;

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

    // World pixels per screen pixel. The node may draw the cell larger or smaller than its
    // 32 pixels, and the edge widths have to follow, or the swatch misreports how thick they
    // are relative to the body.
    let texel = CELL_SIZE / max(in.size.x, 1.0);

    let raw_distance = distance_at(world);
    let d = eroded_distance(raw_distance, world, style);

    let probe = max(LIGHT_PROBE, texel);
    let lit = -(distance_at(world + style.surface.light_direction * probe) - raw_distance) / probe;

    let plate = plate_noise(world, style);
    let colour = wall_shading(d, lit, plate, texel, style);

    return vec4<f32>(colour, smoothstep(-texel, texel, d));
}
