import bevy_sprite_render::mesh2d::vertex_output::VertexOutput;
import package::shaders::common::core::CELL_SIZE;
import package::shaders::common::map_light::MAP_SUN_GROUND_DIRECTION;
import package::shaders::walls::look::{WallStyle, LIGHT_PROBE, eroded_distance, plate_noise, single_cell_distance, wall_shading};

// One isolated wall cell in world space. An analytic box distance replaces the map canvas's
// neighbor-derived field; all surface layers still come from `shaders::walls::look`.

struct WallQuad {
    style: WallStyle,
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

    let raw_distance = single_cell_distance(world);
    let d = eroded_distance(raw_distance, world, settings.style);

    let probe = max(LIGHT_PROBE, texel);
    let lit = -(single_cell_distance(world + MAP_SUN_GROUND_DIRECTION * probe) - raw_distance) / probe;

    let plate = plate_noise(world, settings.style);
    let colour = wall_shading(d, lit, plate, texel, settings.style);

    return vec4<f32>(colour, smoothstep(-texel, texel, d) * settings.alpha);
}
