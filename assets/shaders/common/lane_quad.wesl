import bevy_sprite_render::mesh2d::{vertex_output::VertexOutput, view_bindings::view};

// Cut instance geometry in world space, derived from the entity's transform and quad size.
// Field order and types mirror `LaneQuad` in visuals/src/cut_lane.rs.
struct LaneQuad {
    x_axis: vec2<f32>,
    y_axis: vec2<f32>,
    center: vec2<f32>,
};

// Returns a corner in [-0.5, 0.5] for vertex indices 0..5, forming two triangles.
fn quad_corner(vertex_index: u32) -> vec2<f32> {
    // Bit `vertex_index` of each mask says whether that vertex sits on the positive side.
    let x = f32((0x16u >> vertex_index) & 1u) - 0.5;
    let y = f32((0x34u >> vertex_index) & 1u) - 0.5;
    return vec2<f32>(x, y);
}

// Builds Bevy's 2D vertex output with rectangle UVs: U rightward, V downward.
fn lane_quad_vertex(quad: LaneQuad, vertex_index: u32, instance_index: u32) -> VertexOutput {
    let corner = quad_corner(vertex_index);
    let world = quad.center + quad.x_axis * corner.x + quad.y_axis * corner.y;
    var out: VertexOutput;
    out.position = view.clip_from_world * vec4<f32>(world, 0.0, 1.0);
    out.world_position = vec4<f32>(world, 0.0, 1.0);
    out.world_normal = vec3<f32>(0.0, 0.0, 1.0);
    out.uv = vec2<f32>(corner.x + 0.5, 0.5 - corner.y);
    out.instance_index = instance_index;
    return out;
}
