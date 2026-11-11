// One isolated excavation cell in a UI node.

import bevy_ui_render::ui_vertex_output::UiVertexOutput;
import package::shaders::common::core::CELL_SIZE;
import package::shaders::underground::look::single_cell_excavation;

// Field order and types mirror `ExcavationFaceMaterial` in
// map_objects_internal/src/excavation_materials.rs.
struct ExcavationFace {
    pit: u32, // nonzero for a pit
    shape: u32, // `VolumeShape::gpu_code` when `pit` is zero
}
@group(1) @binding(0) var<uniform> settings: ExcavationFace;

@fragment
fn fragment(in: UiVertexOutput) -> @location(0) vec4<f32> {
    // Flip the downward UI y-axis to match map coordinates and lighting.
    let world = vec2<f32>(in.uv.x, 1.0 - in.uv.y) * CELL_SIZE;

    // World-coordinate span of one screen pixel at the UI node's rendered size.
    let texel = CELL_SIZE / max(in.size.x, 1.0);
    return vec4<f32>(single_cell_excavation(settings.pit, settings.shape, world, texel), 1.0);
}
