// Floor cover entries per cell, uploaded by indexes_internal through the shared GPU group.
// Custom materials enable this group through visuals::shared_gpu; lane shaders include it.

import package::shaders::common::map_info::{grid_contains, grid_index};
import package::shaders::grids::obstacles::{STRUCTURE_BUILDING, STRUCTURE_WALL, VOLUME_ELEVATION, structure_at};

// Field order and types mirror `GpuFloorCoverEntry` in indexes_internal/src/floor_cover.rs.
struct FloorCoverEntry {
    kind: u32,
    // Wrapped game time.
    starts_at: f32,
    // Wrapped game time, or `FLOOR_COVER_PERMANENT_END`.
    ends_at: f32,
    // Floor code (FLOOR_*).
    floor: u32,
};

// Start offset for each cell in `floor_cover_cell_entries`, followed by a final end offset.
@group(3) @binding(112) var<storage, read> floor_cover_cell_offsets: array<u32>;
// Entries in cell order. An effect covering multiple cells has one entry per covered cell.
@group(3) @binding(113) var<storage, read> floor_cover_cell_entries: array<FloorCoverEntry>;

// Mirrors `FloorLevel::gpu_index` in game_core/src/altitude.rs.
const FLOOR_UNDERGROUND: u32 = 0u;
const FLOOR_GROUND: u32 = 1u;
const FLOOR_ELEVATION: u32 = 2u;

// Mirrors `FloorCoverKind::gpu_index` in indexes/src/floor_cover.rs.
const FLOOR_COVER_CHAR: u32 = 0u;
const FLOOR_COVER_NAPALM: u32 = 1u;

// End time of an entry without a lifetime. Mirrors `PERMANENT_END` in indexes_internal/src/floor_cover.rs.
const FLOOR_COVER_PERMANENT_END: f32 = -1.0;

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
    return entry.ends_at == FLOOR_COVER_PERMANENT_END;
}

// Walls and buildings hide floor cover on their cells.
fn floor_cover_masked_at(coords: vec2<i32>) -> bool {
    let structure = structure_at(coords, VOLUME_ELEVATION);
    return structure == STRUCTURE_WALL || structure == STRUCTURE_BUILDING;
}

// Checks for an entry of `kind` on a cell not hidden by a wall or building.
fn floor_cover_shows_kind_at(coords: vec2<i32>, kind: u32) -> bool {
    if floor_cover_masked_at(coords) {
        return false;
    }
    let range = floor_cover_entries_at(coords);
    for (var index = range.x; index < range.y; index++) {
        if floor_cover_cell_entries[index].kind == kind {
            return true;
        }
    }
    return false;
}
