import bevy_sprite_render::mesh2d::vertex_output::VertexOutput;
import package::shaders::common::map_info::grid_bounds;
import package::shaders::grids::emissions::emissions_energy_at;

// Field order and types mirror `EmissionsUniformData` in overlays/src/emissions.rs.
struct EmissionsUniforms {
    min_value: f32,
    max_value: f32,
}

@group(2) @binding(0) var<uniform> uniforms: EmissionsUniforms;

const BACKGROUND_COLOR: vec4<f32> = vec4<f32>(0.0, 0.0, 0.0, 0.498);

@fragment
fn fragment(mesh: VertexOutput) -> @location(0) vec4<f32> {
    let clamped_uv = clamp(mesh.uv, vec2<f32>(0.0), vec2<f32>(0.9999));
    let energy = emissions_energy_at(vec2<i32>(clamped_uv * vec2<f32>(grid_bounds())));

    if (energy == 0.0) {
        return BACKGROUND_COLOR;
    }

    let range = uniforms.max_value - uniforms.min_value;
    var normalized = 0.0;
    if (range > 0.0) {
        normalized = (energy - uniforms.min_value) / range;
    }

    return vec4<f32>(0.0, normalized, normalized, 0.498);
}