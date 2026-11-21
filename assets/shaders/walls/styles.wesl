// Wall cell lookup from `shaders::grids::obstacles` and the shared `WallStyle` table uploaded by
// `map_objects_internal`.
// Custom materials enable this group through `visuals::shared_gpu`; lane shaders include it.

import package::shaders::grids::obstacles::{LEVEL_ON_GROUND, cell_structure_shape, cell_wall_style_index, obstacle_cell_at};
import package::shaders::walls::look::WallStyle;

// The map's style table. Style index 1 is wall_styles[0].
@group(3) @binding(105) var<storage, read> wall_styles: array<WallStyle>;

// The wall in one cell.
struct WallCell {
    // One-based style index, or 0 where no wall stands.
    style: u32,
    // `StructureShape::gpu_code` of the wall, or 0 where no wall stands. Decoded by
    // `shaders::grids::structure_shape`.
    shape: u32,
}

// Returns the wall style and shape at `coords`, or an empty `WallCell(0, 0)` outside the map.
fn wall_cell_at(coords: vec2<i32>) -> WallCell {
    let cell = obstacle_cell_at(coords);
    return WallCell(cell_wall_style_index(cell), cell_structure_shape(cell, LEVEL_ON_GROUND));
}

// Looks up a one-based style index. The caller must ensure it is within 1..=arrayLength(&wall_styles).
fn wall_style(style_index: u32) -> WallStyle {
    return wall_styles[style_index - 1u];
}
