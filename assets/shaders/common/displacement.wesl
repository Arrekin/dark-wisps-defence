// Coordinate conversion for lane displacement shaders.
// Displacement buffer channels are defined in visuals/src/cut_lane.rs.

import bevy_sprite_render::mesh2d::view_bindings::view;

// Converts a world-space sampling offset to screen UV units, including the Y-axis flip.
fn world_offset_to_uv(offset: vec2<f32>) -> vec2<f32> {
    let clip_offset = (view.clip_from_world * vec4<f32>(offset, 0.0, 0.0)).xy;
    return clip_offset * vec2<f32>(0.5, -0.5);
}
