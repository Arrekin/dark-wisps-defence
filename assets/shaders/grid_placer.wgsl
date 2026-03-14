#import bevy_sprite::mesh2d_vertex_output::VertexOutput

// Cell state values packed 2 bits per cell in cell_data:
// 0 = inactive (transparent, outside the imprint shape)
// 1 = active, normal intensity
// 2 = active, highlighted (the base color is made more pronounced)
const CELL_INACTIVE:    u32 = 0u;
const CELL_ACTIVE:      u32 = 1u;
const CELL_HIGHLIGHTED: u32 = 2u;

// Normal cells: how much the tint (base_color) overrides the texture.
// 0.0 = pure texture color, 1.0 = pure base_color tint.
const TINT_STRENGTH: f32 = 0.65;

// Highlighted cells: how much of the texture alpha survives.
// 0.0 = texture invisible (pure tint), 1.0 = full texture alpha.
const HIGHLIGHT_TEXTURE_DEBUFF: f32 = 0.7;

// Highlighted cells: tint override strength (same scale as TINT_STRENGTH).
const HIGHLIGHT_TINT_STRENGTH: f32 = 4.;

struct GridPlacerData {
    base_color: vec4<f32>,
    cell_data: vec4<u32>,  // 2 bits/cell, up to 64 cells (8×8 bounding box)
    cell_columns: u32,
    cell_rows: u32,
    use_texture: u32,
}

@group(2) @binding(0) var<uniform> data: GridPlacerData;
@group(2) @binding(1) var preview_texture: texture_2d<f32>;
@group(2) @binding(2) var preview_sampler: sampler;

fn get_cell_state(ix: u32, iy: u32) -> u32 {
    let cell_index = iy * data.cell_columns + ix;
    let word_index = cell_index / 16u;
    let bit_offset = (cell_index % 16u) * 2u;
    var word: u32;
    switch word_index {
        case 0u:  { word = data.cell_data.x; }
        case 1u:  { word = data.cell_data.y; }
        case 2u:  { word = data.cell_data.z; }
        default:  { word = data.cell_data.w; }
    }
    return (word >> bit_offset) & 3u;
}

@fragment
fn fragment(mesh: VertexOutput) -> @location(0) vec4<f32> {
    let uv = mesh.uv;
    // UV(0,0) is top-left of the mesh (world +Y), so iy=0 maps to UV.y=1.
    let ix = u32(clamp(uv.x * f32(data.cell_columns),  0.0, f32(data.cell_columns)  - 1.0));
    let iy = u32(clamp((1.0 - uv.y) * f32(data.cell_rows), 0.0, f32(data.cell_rows) - 1.0));

    let state = get_cell_state(ix, iy);
    if state == CELL_INACTIVE {
        return vec4<f32>(0.0);
    }

    var color: vec4<f32>;
    if data.use_texture != 0u {
        let tex = textureSampleLevel(preview_texture, preview_sampler, uv, 0.0);
        var tint_weight: f32 = TINT_STRENGTH;
        var tex_alpha: f32 = tex.a;
        if state == CELL_HIGHLIGHTED {
            tint_weight = HIGHLIGHT_TINT_STRENGTH;
            tex_alpha = tex_alpha * HIGHLIGHT_TEXTURE_DEBUFF;
        }
        color = vec4<f32>(mix(tex.rgb, data.base_color.rgb, tint_weight), tex_alpha * 0.5);
    } else {
        color = data.base_color;
    }

    return color;
}
