// Full-screen displacement and scene-copy passes used at band boundaries.
// Displacement buffer channels are defined in visuals/src/cut_lane.rs.

import bevy_core_pipeline::fullscreen_vertex_shader::FullscreenVertexOutput;

@group(0) @binding(0) var scene_texture: texture_2d<f32>;
@group(0) @binding(1) var scene_sampler: sampler;
@group(0) @binding(2) var displacement_texture: texture_2d<f32>;

// Applies a band's displacement buffer to the scene drawn so far.
@fragment
fn apply_displacement(in: FullscreenVertexOutput) -> @location(0) vec4<f32> {
    let displacement = textureLoad(displacement_texture, vec2<i32>(in.position.xy), 0);
    let scene = textureSampleLevel(scene_texture, scene_sampler, in.uv + displacement.xy, 0.0);
    return vec4<f32>(scene.rgb * (1.0 + displacement.z), scene.a);
}

// Copies the scene drawn so far into the other main texture, so overlays can draw onto the copy
// while sampling the original. Binds only the scene texture.
@fragment
fn copy_scene(in: FullscreenVertexOutput) -> @location(0) vec4<f32> {
    return textureLoad(scene_texture, vec2<i32>(in.position.xy), 0);
}
