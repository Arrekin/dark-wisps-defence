// Samples the scene captured after a band's displacement and before its overlays.
// Bindings match `BENEATH_TEXTURE_BINDING` and `BENEATH_SAMPLER_BINDING` in
// visuals/src/shared_gpu.rs. When no snapshot is bound, sampling reads Bevy's fallback image.

import bevy_sprite_render::mesh2d::view_bindings::view;

@group(3) @binding(110) var beneath_texture: texture_2d<f32>;
@group(3) @binding(111) var beneath_sampler: sampler;

// Maps a world position to screen UV coordinates, with V increasing downward.
fn world_to_uv(world: vec2<f32>) -> vec2<f32> {
    let clip = (view.clip_from_world * vec4<f32>(world, 0.0, 1.0)).xy;
    return clip * vec2<f32>(0.5, -0.5) + vec2<f32>(0.5, 0.5);
}

// Samples the captured scene at a world position.
fn beneath_at_world(world: vec2<f32>) -> vec4<f32> {
    return textureSampleLevel(beneath_texture, beneath_sampler, world_to_uv(world), 0.0);
}
