// Obstacle cells uploaded by grids_internal through the shared GPU group.
// Custom materials enable this group through visuals::shared_gpu; lane shaders include it.

import package::shaders::common::map_info::{grid_contains, grid_index};

// One packed u32 per cell, row major. The layout mirrors `pack_obstacle_cell` in
// grids_internal/src/obstacles.rs:
//   bits 0-1   structure kind (STRUCTURE_*)
//   bit 2      dark ore present
//   bit 3      quantum field present
//   bits 8-15  building type (BUILDING_*), 0 when the cell holds no building
@group(3) @binding(103) var<storage, read> obstacle_cells: array<u32>;

const STRUCTURE_EMPTY: u32 = 0u;
const STRUCTURE_WALL: u32 = 1u;
const STRUCTURE_BUILDING: u32 = 2u;
// Never stored. Returned for coordinates outside the map.
const STRUCTURE_OFF_MAP: u32 = 3u;

const STRUCTURE_MASK: u32 = 3u;
const DARK_ORE_BIT: u32 = 4u;
const QUANTUM_FIELD_BIT: u32 = 8u;
const BUILDING_TYPE_SHIFT: u32 = 8u;

// Mirrors `BuildingType::gpu_index` in game_core/src/types.rs.
const BUILDING_NONE: u32 = 0u;
const BUILDING_MAIN_BASE: u32 = 1u;
const BUILDING_ENERGY_RELAY: u32 = 2u;
const BUILDING_MINING_COMPLEX: u32 = 3u;
const BUILDING_EXPLORATION_CENTER: u32 = 4u;
const BUILDING_FORGE: u32 = 5u;
const BUILDING_TOWER_FIRST: u32 = 32u;
const TOWER_BLASTER: u32 = 32u;
const TOWER_CANNON: u32 = 33u;
const TOWER_ROCKET_LAUNCHER: u32 = 34u;
const TOWER_EMITTER: u32 = 35u;
const TOWER_FIELD: u32 = 36u;
const TOWER_FLAMETHROWER: u32 = 37u;
const TOWER_BLAZE_MORTAR: u32 = 38u;
const TOWER_INFERNO: u32 = 39u;

// Returns the packed cell, or STRUCTURE_OFF_MAP with no other bits set outside the map.
fn obstacle_cell_at(coords: vec2<i32>) -> u32 {
    if !grid_contains(coords) {
        return STRUCTURE_OFF_MAP;
    }
    return obstacle_cells[grid_index(coords)];
}

fn structure_at(coords: vec2<i32>) -> u32 {
    return obstacle_cell_at(coords) & STRUCTURE_MASK;
}

fn building_type_at(coords: vec2<i32>) -> u32 {
    return (obstacle_cell_at(coords) >> BUILDING_TYPE_SHIFT) & 0xFFu;
}

fn is_tower_at(coords: vec2<i32>) -> bool {
    return building_type_at(coords) >= BUILDING_TOWER_FIRST;
}

fn has_dark_ore_at(coords: vec2<i32>) -> bool {
    return (obstacle_cell_at(coords) & DARK_ORE_BIT) != 0u;
}

fn is_within_quantum_field_at(coords: vec2<i32>) -> bool {
    return (obstacle_cell_at(coords) & QUANTUM_FIELD_BIT) != 0u;
}

// Classifies structures, dark ore and out-of-map coordinates as raised ground.
fn is_raised_at(coords: vec2<i32>) -> bool {
    let cell = obstacle_cell_at(coords);
    return (cell & STRUCTURE_MASK) != STRUCTURE_EMPTY || (cell & DARK_ORE_BIT) != 0u;
}
