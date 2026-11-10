// Wall styles and packed cell data uploaded by map_objects_internal through the shared GPU group.
// Custom materials enable this group through visuals::shared_gpu; lane shaders include it.

import package::shaders::common::map_info::{grid_contains, grid_index};
import package::shaders::walls::look::WallStyle;

// One u32 per cell in row-major order. Bits 0–7 hold the one-based style index, or 0 for open ground.
// Bits 8–14 hold the shape code from `WallShape::gpu_code`.
// The layout mirrors `upload_gpu_wall_cells` in map_objects_internal/src/wall_canvas.rs.
//
// Within the shape code, bits 0–2 hold the ramp code: 0 for flat, 1–4 for right, left, top, and bottom.
// Bits 3–6 hold the tunnel openings (OPENING_*), or zero for a ramp.
@group(3) @binding(104) var<storage, read> wall_cells: array<u32>;
// The map's style table. Style index 1 is wall_styles[0].
@group(3) @binding(105) var<storage, read> wall_styles: array<WallStyle>;

// Mirrors `SHAPE_SHIFT` in map_objects_internal/src/wall_canvas.rs.
const SHAPE_SHIFT: u32 = 8u;
const STYLE_MASK: u32 = 0xFFu;
const RAMP_MASK: u32 = 0x7u;
const OPENINGS_SHIFT: u32 = 3u;

// Opening mask returned by `shape_openings`; matches `EdgeSides::bits` in game_core/src/grid.rs.
const OPENING_TOP: u32 = 1u;
const OPENING_RIGHT: u32 = 2u;
const OPENING_BOTTOM: u32 = 4u;
const OPENING_LEFT: u32 = 8u;

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

// Returns the shape code. Zero represents a solid flat wall or open ground.
fn cell_shape(cell: u32) -> u32 {
    return cell >> SHAPE_SHIFT;
}

// Returns the ramp code, or 0 for a flat wall.
fn shape_ramp(shape: u32) -> u32 {
    return shape & RAMP_MASK;
}

// Returns the OPENING_* bits of a flat wall, or 0 for a ramp.
fn shape_openings(shape: u32) -> u32 {
    return shape >> OPENINGS_SHIFT;
}

// Tests the opening mask from `shape_openings` in cardinal direction `side`; positive y points up.
fn opens_toward(openings: u32, side: vec2<i32>) -> bool {
    var bit = OPENING_LEFT;
    if side.y > 0 {
        bit = OPENING_TOP;
    } else if side.x > 0 {
        bit = OPENING_RIGHT;
    } else if side.y < 0 {
        bit = OPENING_BOTTOM;
    }
    return (openings & bit) != 0u;
}

// Returns the ramp code, or 0 for a flat cell.
fn cell_ramp(cell: u32) -> u32 {
    return shape_ramp(cell_shape(cell));
}

// Returns the OPENING_* bits of a flat wall, or 0 for open ground and ramps.
fn cell_openings(cell: u32) -> u32 {
    return shape_openings(cell_shape(cell));
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
