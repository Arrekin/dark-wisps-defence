import bevy_sprite_render::mesh2d::{vertex_output::VertexOutput, view_bindings::globals};
import package::shaders::common::map_light::{MAP_SUN_ELEVATION_TANGENT, MAP_SUN_GROUND_DIRECTION};
import package::shaders::void_sea::waves::{WATER_HEIGHT, map_size, wall_coordinate, wall_lift};

// The shadow the sea casts onto the map along the edges facing the map sun. It lengthens where a wave
// climbs the glass. The output alpha darkens what lies beneath: the pipeline's blend multiplies the
// destination by one minus it.

// How much of the light the sea's shadow takes away.
const SHADOW_STRENGTH: f32 = 0.6;

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    let p = in.world_position.xy;
    let size = map_size();
    // Distance toward the sun until the ray leaves the map.
    let toward_sun = MAP_SUN_GROUND_DIRECTION;
    let exit_x = select((size.x - p.x) / toward_sun.x, p.x / -toward_sun.x, toward_sun.x < 0.0);
    let exit_y = select(p.y / -toward_sun.y, (size.y - p.y) / toward_sun.y, toward_sun.y > 0.0);
    let to_edge = min(exit_x, exit_y);
    // Just past the edge, so the point lies on the sea's side of the glass.
    let edge_point = p + toward_sun * (to_edge + 0.5);
    let height = WATER_HEIGHT + wall_lift(wall_coordinate(edge_point), globals.time);
    let shadow_length = height / MAP_SUN_ELEVATION_TANGENT;
    let shadow = 1.0 - smoothstep(shadow_length * 0.8, shadow_length, to_edge);
    return vec4<f32>(0.0, 0.0, 0.0, shadow * SHADOW_STRENGTH);
}
