// Renders an isolated excavation cell for placement previews.

import bevy_sprite_render::mesh2d::vertex_output::VertexOutput;
import package::shaders::common::core::CELL_SIZE;
import package::shaders::underground::look::single_cell_excavation;

// Field order and types mirror `ExcavationQuadMaterial` in
// map_objects_internal/src/excavation_materials.rs.
struct ExcavationQuad {
    alpha: f32,
    pit: u32, // nonzero for a pit
    shape: u32, // `VolumeShape::gpu_code` when `pit` is zero
}
@group(2) @binding(0) var<uniform> settings: ExcavationQuad;

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    // Flip the downward UV y-axis to match map coordinates and lighting.
    let world = vec2<f32>(in.uv.x, 1.0 - in.uv.y) * CELL_SIZE;

    // World-coordinate span of one screen pixel at the quad's rendered size.
    let texel = max(fwidth(world.x), 0.001);
    return vec4<f32>(single_cell_excavation(settings.pit, settings.shape, world, texel), settings.alpha);
}
