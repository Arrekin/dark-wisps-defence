#import bevy_sprite::mesh2d_vertex_output::VertexOutput

// Tower Ranges Overlay Shader
// - TowerRangeCell channels:
//   signature: u32    // XOR of tower entity indices covering this cell
//   cover_count: u32  // number of towers covering this cell
//   highlight: u32    // 1 if highlighted (selected or preview), else 0
//
// Edge drawing policy:
// - Highlight edges: draw when current cell is highlighted and neighbor is not.
// - Signature edges: draw when signatures differ and current cell has coverage (>0),
//   with tie-breaking:
//   * draw from the higher cover_count side
//   * if equal and == 1, allow both sides (two single ranges meeting)
//   * if equal and > 1, draw only from the side with greater signature (stable tiebreaker)
//
// Dash pattern:
// - Two gaps per edge at 1/3 and 2/3. Endpoints remain on to keep corners clean.
// - GAP_HALF controls the half-width of each gap.
//
struct TowerRangeCell {
    signature: u32,
    cover_count: u32,
    highlight: u32,
}

struct UniformGridData {
    grid_width: u32,
    grid_height: u32,
}

@group(2) @binding(0) var<storage, read> cells: array<TowerRangeCell>;
@group(2) @binding(1) var<uniform> uniforms: UniformGridData;

// Rendering constants
const blockSize: f32 = 16.0; // Size of each block in pixels
const outlineThickness: f32 = 2.0; // Size of the outline in pixels
const outlineRatio: f32 = outlineThickness / blockSize; // Outline thickness relative to cell size

// Dash segmentation: fixed 2-gap pattern per cell edge; each gap half-width in [0..1] of the cell edge
const GAP_HALF: f32 = 0.070;

const BASE_COLOR: vec4<f32> = vec4<f32>(0.0, 0.0, 0.0, 0.0); // Transparent
const OUTLINE_COLOR: vec4<f32> = vec4<f32>(1.0, 1.0, 1.0, 0.03); // White, dim
const SELECTED_OUTLINE_COLOR: vec4<f32> = vec4<f32>(0.0, 1.0, 1.0, 0.9); // Cyan bright outline

fn get_cell_data(uv: vec2<f32>) -> TowerRangeCell {
    // Clamp UV coordinates to valid range to prevent bleeding at edges
    let clamped_uv = clamp(uv, vec2<f32>(0.0), vec2<f32>(0.9999));
    let grid_pos = vec2<u32>(clamped_uv * vec2<f32>(f32(uniforms.grid_width), f32(uniforms.grid_height)));
    let index = grid_pos.y * uniforms.grid_width + grid_pos.x;

    // Additional safety check: ensure grid coordinates are within bounds
    if (grid_pos.x >= uniforms.grid_width || grid_pos.y >= uniforms.grid_height || index >= arrayLength(&cells)) {
        return TowerRangeCell(0u, 0u, 0u); // Return empty cell if out of bounds
    }

    return cells[index];
}

// Removed unused EdgeInfo and helper functions; fragment() performs direct neighbor checks.

fn dashed_mask_oriented(blockPosition: vec2<f32>, vertical: bool) -> bool {
    // Static per-cell pattern: 2 evenly spaced gaps at 1/3 and 2/3 along the edge.
    // Endpoints (0 and 1) are always ON for clean joins across cells and at corners.
    let along = select(blockPosition.x, blockPosition.y, vertical);
    let gap1 = abs(along - (1.0 / 3.0)) < GAP_HALF;
    let gap2 = abs(along - (2.0 / 3.0)) < GAP_HALF;
    return !(gap1 || gap2);
}

@fragment
fn fragment(mesh: VertexOutput) -> @location(0) vec4<f32> {
    let uv = mesh.uv;
    
    // Position within the block (cell)
    let blockPosition = fract(uv * vec2<f32>(f32(uniforms.grid_width), f32(uniforms.grid_height)));

    // Check if we're at the very edge of a block
    let atEdge = blockPosition.x <= outlineRatio || blockPosition.x >= (1.0 - outlineRatio) ||
                 blockPosition.y <= outlineRatio || blockPosition.y >= (1.0 - outlineRatio);

                     // Get cell data from buffer
    let cell = get_cell_data(uv);

    // Early out: empty cells contribute no edges (edges are drawn from the inside cell) and we are only interested in edges
    if (!atEdge || (cell.cover_count == 0u && cell.highlight == 0u)) {
        return BASE_COLOR;
    }

    // Reuse highlight flag
    let curr_is_highlight = (cell.highlight != 0u);

    let stepSize = vec2<f32>(1.0 / f32(uniforms.grid_width), 1.0 / f32(uniforms.grid_height));
    let in_vertical_band = (blockPosition.x <= outlineRatio) || (blockPosition.x >= (1.0 - outlineRatio));
    let in_horizontal_band = (blockPosition.y <= outlineRatio) || (blockPosition.y >= (1.0 - outlineRatio));

    // Vertical orientation: sample left/right neighbor
    var draw_highlight_v = false;
    var draw_signature_v = false;
    var dash_v = false;
    if (in_vertical_band) {
        let is_right_band = blockPosition.x >= (1.0 - outlineRatio);
        let neighbor_uv_v = uv + vec2<f32>(select(-stepSize.x, stepSize.x, is_right_band), 0.0);
        let neighbor_v = get_cell_data(neighbor_uv_v);
        let curr_highlight_v = curr_is_highlight;
        let neigh_highlight_v = (neighbor_v.highlight != 0u);
        draw_highlight_v = curr_highlight_v && !neigh_highlight_v;
        // Draw signature edge when signatures differ. For outer edges (neighbor==0), this stays single-sided
        // because we require current cell to be inside (cell.signature != 0u). For inter-range edges (both non-zero),
        // each side draws its own half, which is acceptable and desired to show all ranges.
        // Signature edge policy:
        // - If cover counts differ, draw from the higher-count side (single-sided in overlaps: 2 vs 1, etc.).
        // - If counts are equal and == 1, allow both sides (two single ranges meeting -> acceptable double thickness).
        // - If counts are equal and > 1, use deterministic tiebreaker to avoid double inside multi-overlaps.
        draw_signature_v = (cell.signature != neighbor_v.signature)
                            && (cell.cover_count > 0u)
                            && (
                                (cell.cover_count > neighbor_v.cover_count)
                                || (cell.cover_count == neighbor_v.cover_count && cell.cover_count == 1u)
                                || (cell.cover_count == neighbor_v.cover_count && cell.cover_count > 1u && cell.signature > neighbor_v.signature)
                            );
        dash_v = dashed_mask_oriented(blockPosition, true);
    }

    // Horizontal orientation: sample below/above neighbor
    var draw_highlight_h = false;
    var draw_signature_h = false;
    var dash_h = false;
    if (in_horizontal_band) {
        let is_top_band = blockPosition.y >= (1.0 - outlineRatio);
        let neighbor_uv_h = uv + vec2<f32>(0.0, select(-stepSize.y, stepSize.y, is_top_band));
        let neighbor_h = get_cell_data(neighbor_uv_h);
        let curr_highlight_h = curr_is_highlight;
        let neigh_highlight_h = (neighbor_h.highlight != 0u);
        draw_highlight_h  = curr_highlight_h && !neigh_highlight_h;
        // Same logic for horizontal edges
        draw_signature_h = (cell.signature != neighbor_h.signature)
                            && (cell.cover_count > 0u)
                            && (
                                (cell.cover_count > neighbor_h.cover_count)
                                || (cell.cover_count == neighbor_h.cover_count && cell.cover_count == 1u)
                                || (cell.cover_count == neighbor_h.cover_count && cell.cover_count > 1u && cell.signature > neighbor_h.signature)
                            );
        dash_h = dashed_mask_oriented(blockPosition, false);
    }

    // Priority: highlight > signature. Combine vertical/horizontal (corners) with OR.
    var draw_highlight_any = (draw_highlight_v && dash_v) || (draw_highlight_h && dash_h);
    var draw_signature_any = ((draw_signature_v && dash_v) || (draw_signature_h && dash_h)) && !draw_highlight_any;

    // Corner fix: force fill at corner junctions for symmetry; if only diagonal neighbor is inside,
    // draw a single pixel so corners don't look hollow regardless of dash mask.
    if (in_vertical_band && in_horizontal_band && !(draw_highlight_any || draw_signature_any)) {
        let is_right_band = blockPosition.x >= (1.0 - outlineRatio);
        let is_top_band = blockPosition.y >= (1.0 - outlineRatio);
        let neighbor_uv_d = uv + vec2<f32>(select(-stepSize.x, stepSize.x, is_right_band),
                                            select(-stepSize.y, stepSize.y, is_top_band));
        let neighbor_d = get_cell_data(neighbor_uv_d);
        let curr_highlight_d = curr_is_highlight;
        let neigh_highlight_d = (neighbor_d.highlight != 0u);
        let draw_highlight_d = curr_highlight_d && !neigh_highlight_d;
        let draw_signature_d = (cell.signature != neighbor_d.signature)
                                && (cell.cover_count > 0u)
                                && (
                                    (cell.cover_count > neighbor_d.cover_count)
                                    || (cell.cover_count == neighbor_d.cover_count && cell.cover_count == 1u)
                                    || (cell.cover_count == neighbor_d.cover_count && cell.cover_count > 1u && cell.signature > neighbor_d.signature)
                                );
        if (draw_highlight_d) {
            draw_highlight_any = true;
        } else if (draw_signature_d) {
            draw_signature_any = true;
        }
    }

    if (draw_highlight_any) {
        return SELECTED_OUTLINE_COLOR;
    } else if (draw_signature_any) {
        return OUTLINE_COLOR;
    } else {
        return BASE_COLOR;
    }
}
