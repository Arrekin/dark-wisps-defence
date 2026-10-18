// Map bounds uploaded by game_core_internal through the shared GPU group.
// Custom materials enable this group through visuals::shared_gpu; lane shaders include it.

import package::shaders::common::core::{bounds_contains, bounds_index};

// Field order and types mirror `GpuMapInfo` in game_core_internal/src/lib.rs.
struct GpuMapInfo {
    grid_bounds: vec2<u32>,
};

@group(3) @binding(100) var<storage, read> map_info: GpuMapInfo;

fn grid_bounds() -> vec2<u32> {
    return map_info.grid_bounds;
}

fn grid_contains(coords: vec2<i32>) -> bool {
    return bounds_contains(coords, grid_bounds());
}

// Row-major cell index. The caller must ensure `grid_contains(coords)` is true.
fn grid_index(coords: vec2<i32>) -> u32 {
    return bounds_index(coords, grid_bounds());
}
