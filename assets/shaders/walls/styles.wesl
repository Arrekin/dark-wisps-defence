// Wall styles and packed cell data uploaded by map_objects_internal through the shared GPU group.
// Custom materials enable this group through visuals::shared_gpu; lane shaders include it.

import package::shaders::common::map_info::{grid_contains, grid_index};
import package::shaders::walls::look::WallStyle;

// One u32 per cell in row-major order. The low 8 bits hold the one-based style index, or 0 for
// open ground. The upper bits hold the ramp code: 0 for flat cells, 1–4 for an uphill direction.
// The layout mirrors `upload_gpu_wall_cells` in map_objects_internal/src/wall_canvas.rs.
@group(3) @binding(104) var<storage, read> wall_cells: array<u32>;
// The map's style table. Style index 1 is wall_styles[0].
@group(3) @binding(105) var<storage, read> wall_styles: array<WallStyle>;

// Mirrors `RAMP_CODE_SHIFT` in map_objects_internal/src/wall_canvas.rs.
const RAMP_CODE_SHIFT: u32 = 8u;
const STYLE_MASK: u32 = 0xFFu;

// Returns the packed cell value, or 0 (open ground) outside the map.
fn wall_cell_at(coords: vec2<i32>) -> u32 {
    if !grid_contains(coords) {
        return 0u;
    }
    return wall_cells[grid_index(coords)];
}

// Returns the one-based style index, or 0 for open ground.
fn cell_style(cell: u32) -> u32 {
    return cell & STYLE_MASK;
}

// Returns the ramp code, or 0 for a flat cell.
fn cell_ramp(cell: u32) -> u32 {
    return cell >> RAMP_CODE_SHIFT;
}

// Returns the one-based style index at `coords`, or 0 for open ground and coordinates outside the map.
fn wall_style_index_at(coords: vec2<i32>) -> u32 {
    return cell_style(wall_cell_at(coords));
}

// Returns the ramp code at `coords`, or 0 for flat cells and coordinates outside the map.
fn wall_ramp_at(coords: vec2<i32>) -> u32 {
    return cell_ramp(wall_cell_at(coords));
}

// Looks up a one-based style index. The caller must ensure it is within 1..=arrayLength(&wall_styles).
fn wall_style(style_index: u32) -> WallStyle {
    return wall_styles[style_index - 1u];
}
