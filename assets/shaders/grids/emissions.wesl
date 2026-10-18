// Emitted energy uploaded by grids_internal through the shared GPU group.
// Custom materials enable this group through visuals::shared_gpu; lane shaders include it.

import package::shaders::common::map_info::{grid_contains, grid_index};

// One f32 per cell, row major: the emitted energy reaching the cell.
@group(3) @binding(106) var<storage, read> emissions_energy: array<f32>;

// Returns the cell's emitted energy, or zero outside the map.
fn emissions_energy_at(coords: vec2<i32>) -> f32 {
    if !grid_contains(coords) {
        return 0.0;
    }
    return emissions_energy[grid_index(coords)];
}
