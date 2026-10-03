#import bevy_sprite::mesh2d_vertex_output::VertexOutput

struct EmissionsCell {
    energy: f32,
}

struct EmissionsUniforms {
    grid_width: u32,
    grid_height: u32,
    min_value: f32,
    max_value: f32,
}

@group(2) @binding(0) var<storage, read> cells: array<EmissionsCell>;
@group(2) @binding(1) var<uniform> uniforms: EmissionsUniforms;

const BACKGROUND_COLOR: vec4<f32> = vec4<f32>(0.0, 0.0, 0.0, 0.498);

fn get_cell_data(uv: vec2<f32>) -> EmissionsCell {
    let clamped_uv = clamp(uv, vec2<f32>(0.0), vec2<f32>(0.9999));
    let grid_pos = vec2<u32>(clamped_uv * vec2<f32>(f32(uniforms.grid_width), f32(uniforms.grid_height)));
    let index = grid_pos.y * uniforms.grid_width + grid_pos.x;

    if (grid_pos.x >= uniforms.grid_width || grid_pos.y >= uniforms.grid_height || index >= arrayLength(&cells)) {
        return EmissionsCell(0.0);
    }

    return cells[index];
}

@fragment
fn fragment(mesh: VertexOutput) -> @location(0) vec4<f32> {
    let cell = get_cell_data(mesh.uv);

    if (cell.energy == 0.0) {
        return BACKGROUND_COLOR;
    }

    let range = uniforms.max_value - uniforms.min_value;
    var normalized = 0.0;
    if (range > 0.0) {
        normalized = (cell.energy - uniforms.min_value) / range;
    }

    return vec4<f32>(0.0, normalized, normalized, 0.498);
}