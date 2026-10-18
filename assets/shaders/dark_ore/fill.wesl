// Dark ore fill uploaded by map_objects_internal through the shared GPU group.
// Custom materials enable this group through visuals::shared_gpu; lane shaders include it.

import package::shaders::common::map_info::{grid_contains, grid_index};

// One f32 per cell, row major: the normalised fill level, 0.0 meaning no ore.
@group(3) @binding(102) var<storage, read> dark_ore_fill: array<f32>;

// Returns the cell's fill level, or zero outside the map.
fn dark_ore_fill_at(coords: vec2<i32>) -> f32 {
    if !grid_contains(coords) {
        return 0.0;
    }
    return dark_ore_fill[grid_index(coords)];
}
