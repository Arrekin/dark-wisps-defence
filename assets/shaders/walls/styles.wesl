// Wall styles and cell indices uploaded by map_objects_internal through the shared GPU group.
// Custom materials enable this group through visuals::shared_gpu; lane shaders include it.

import package::shaders::common::map_info::{grid_contains, grid_index};
import package::shaders::walls::look::WallStyle;

// One u32 per cell, row major: 0 for open ground, otherwise the one-based style index.
@group(3) @binding(104) var<storage, read> wall_style_cells: array<u32>;
// The map's style table. Style index 1 is wall_styles[0].
@group(3) @binding(105) var<storage, read> wall_styles: array<WallStyle>;

// The one-based style index of the wall in a cell. Open ground and cells off the map read as 0.
fn wall_style_index_at(coords: vec2<i32>) -> u32 {
    if !grid_contains(coords) {
        return 0u;
    }
    return wall_style_cells[grid_index(coords)];
}

// Looks up a one-based style index. The caller must ensure it is within 1..=arrayLength(&wall_styles).
fn wall_style(style_index: u32) -> WallStyle {
    return wall_styles[style_index - 1u];
}
