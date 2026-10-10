
// Mapping between full-screen UVs and world space for post-process passes. UV y runs down and world
// y runs up, so every pass that samples the screen at a world position must flip the same way.

// The part of the camera the mapping needs. Each pass builds it from its own camera uniform.
struct ScreenView {
    world_pos: vec2<f32>,
    viewport_size: vec2<f32>,
};

fn screen_uv_to_world(uv: vec2<f32>, view: ScreenView) -> vec2<f32> {
    let centered = uv - vec2<f32>(0.5, 0.5);
    return view.world_pos + centered * view.viewport_size * vec2<f32>(1.0, -1.0);
}

fn screen_world_to_uv(world: vec2<f32>, view: ScreenView) -> vec2<f32> {
    let centered = (world - view.world_pos) / view.viewport_size;
    return centered * vec2<f32>(1.0, -1.0) + vec2<f32>(0.5, 0.5);
}
