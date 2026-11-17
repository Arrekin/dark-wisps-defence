// Floor cover entries per cell, uploaded by indexes_internal through the shared GPU group.
// Custom materials enable this group through visuals::shared_gpu; lane shaders include it.

import package::shaders::common::clock::ClockInstant;
import package::shaders::common::map_info::{grid_contains, grid_index};

// Field order and types mirror `GpuFloorCoverEntry` in indexes_internal/src/floor_cover.rs.
struct FloorCoverEntry {
    kind: u32,
    // Game-clock instant when the cover started.
    start: ClockInstant,
    // Lifetime in seconds, or `FLOOR_COVER_PERMANENT_DURATION`.
    duration: f32,
    // Level code (LEVEL_* in grids/obstacles.wesl).
    level: u32,
};

// Start offset for each cell in `floor_cover_cell_entries`, followed by a final end offset.
@group(3) @binding(112) var<storage, read> floor_cover_cell_offsets: array<u32>;
// Entries in cell order. An effect covering multiple cells has one entry per covered cell.
@group(3) @binding(113) var<storage, read> floor_cover_cell_entries: array<FloorCoverEntry>;

// Mirrors `FloorCoverKind::gpu_index` in indexes/src/floor_cover.rs.
const FLOOR_COVER_CHAR: u32 = 0u;
const FLOOR_COVER_NAPALM: u32 = 1u;

// Duration sentinel for entries without `TimeBound`. Mirrors `PERMANENT_DURATION` in indexes_internal/src/floor_cover.rs.
const FLOOR_COVER_PERMANENT_DURATION: f32 = -1.0;

// Returns the start and end offsets of the cell's entries in `floor_cover_cell_entries`.
// Returns an empty range outside the map.
fn floor_cover_entries_at(coords: vec2<i32>) -> vec2<u32> {
    if !grid_contains(coords) {
        return vec2<u32>(0u);
    }
    let cell = grid_index(coords);
    return vec2<u32>(floor_cover_cell_offsets[cell], floor_cover_cell_offsets[cell + 1u]);
}

fn floor_cover_is_permanent(entry: FloorCoverEntry) -> bool {
    return entry.duration == FLOOR_COVER_PERMANENT_DURATION;
}

// Checks for an entry of `kind` on the floor of `level` at `coords`.
fn floor_cover_has_kind_at(coords: vec2<i32>, kind: u32, level: u32) -> bool {
    let range = floor_cover_entries_at(coords);
    for (var index = range.x; index < range.y; index++) {
        let entry = floor_cover_cell_entries[index];
        if entry.kind == kind && entry.level == level {
            return true;
        }
    }
    return false;
}
