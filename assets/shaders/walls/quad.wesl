// Renders an isolated wall cell for placement previews, with an entrance notch at each opening.

import bevy_sprite_render::mesh2d::vertex_output::VertexOutput;
import package::shaders::common::core::CELL_SIZE;
import package::shaders::walls::look::{mouth_toward, nearer_mouth, no_mouth, single_cell_ramp, single_cell_wall};
import package::shaders::walls::styles::{opens_toward, shape_openings, shape_ramp, wall_style};

// Field order and types mirror `WallQuadMaterial` in map_objects_internal/src/wall_materials.rs.
struct WallQuad {
    style_index: u32, // one-based index into the shared wall style table
    alpha: f32,
    shape: u32, // `WallShape::gpu_code`
}
@group(2) @binding(0) var<uniform> settings: WallQuad;

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    // Mesh UV has y running down and the canvas has it running up. Flipping here keeps the
    // light on the same side of the wall as it is on the map.
    let world = vec2<f32>(in.uv.x, 1.0 - in.uv.y) * CELL_SIZE;

    // World-coordinate span of one screen pixel at the quad's rendered size.
    let texel = max(fwidth(world.x), 0.001);
    let style = wall_style(settings.style_index);
    var wall: vec4<f32>;
    let ramp = shape_ramp(settings.shape);
    if ramp != 0u {
        wall = single_cell_ramp(world, texel, ramp, style);
    } else {
        let openings = shape_openings(settings.shape);
        let p = world / CELL_SIZE - vec2<f32>(0.5);
        var mouth = no_mouth();
        var sides = array<vec2<i32>, 4>(vec2<i32>(0, 1), vec2<i32>(1, 0), vec2<i32>(0, -1), vec2<i32>(-1, 0));
        for (var i = 0; i < 4; i++) {
            if opens_toward(openings, sides[i]) {
                mouth = nearer_mouth(mouth, mouth_toward(p, vec2<f32>(sides[i])));
            }
        }
        wall = single_cell_wall(world, texel, mouth, style);
    }
    return vec4<f32>(wall.rgb, wall.a * settings.alpha);
}
