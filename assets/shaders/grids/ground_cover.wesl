// Ground cover entries per cell, uploaded by grids_internal through the shared GPU group.
// Custom materials enable this group through visuals::shared_gpu; lane shaders include it.

import package::shaders::common::map_info::{grid_contains, grid_index};
import package::shaders::grids::obstacles::{STRUCTURE_BUILDING, STRUCTURE_WALL, structure_at};

// Field order and types mirror `GpuGroundCoverEntry` in grids_internal/src/ground_cover.rs.
struct GroundCoverEntry {
    kind: u32,
    // Wrapped game time.
    starts_at: f32,
    // Wrapped game time, or `GROUND_COVER_PERMANENT_END`.
    ends_at: f32,
    _padding: u32,
};

// Start offset for each cell in `ground_cover_cell_entries`, followed by a final end offset.
@group(3) @binding(112) var<storage, read> ground_cover_cell_offsets: array<u32>;
// Entries concatenated in cell order. An entry covering several cells repeats in each of them.
@group(3) @binding(113) var<storage, read> ground_cover_cell_entries: array<GroundCoverEntry>;

// Mirrors `GroundCoverKind` in grids/src/ground_cover.rs.
const GROUND_COVER_CHAR: u32 = 0u;
const GROUND_COVER_NAPALM: u32 = 1u;

// End time of an entry without a lifetime. Mirrors `PERMANENT_END` in grids_internal/src/ground_cover.rs.
const GROUND_COVER_PERMANENT_END: f32 = -1.0;

// Returns the start and end offsets of the cell's entries in `ground_cover_cell_entries`.
// Returns an empty range outside the map.
fn ground_cover_entries_at(coords: vec2<i32>) -> vec2<u32> {
    if !grid_contains(coords) {
        return vec2<u32>(0u);
    }
    let cell = grid_index(coords);
    return vec2<u32>(ground_cover_cell_offsets[cell], ground_cover_cell_offsets[cell + 1u]);
}

fn ground_cover_is_permanent(entry: GroundCoverEntry) -> bool {
    return entry.ends_at == GROUND_COVER_PERMANENT_END;
}

// Walls and buildings hide ground cover on their cells.
fn ground_cover_masked_at(coords: vec2<i32>) -> bool {
    let structure = structure_at(coords);
    return structure == STRUCTURE_WALL || structure == STRUCTURE_BUILDING;
}

// Checks for an entry of `kind` on a cell not hidden by a wall or building.
fn ground_cover_shows_kind_at(coords: vec2<i32>, kind: u32) -> bool {
    if ground_cover_masked_at(coords) {
        return false;
    }
    let range = ground_cover_entries_at(coords);
    for (var index = range.x; index < range.y; index++) {
        if ground_cover_cell_entries[index].kind == kind {
            return true;
        }
    }
    return false;
}
