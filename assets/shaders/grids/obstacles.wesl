// Obstacle cells uploaded by indexes_internal through the shared GPU group.
// Custom materials enable this group through visuals::shared_gpu; lane shaders include it.

import package::shaders::common::map_info::{grid_contains, grid_index};

// One packed u32 per cell, row major. The layout mirrors `pack_obstacle_cell` in
// indexes_internal/src/obstacles.rs:
//   bits 0-3    underground structure kind (STRUCTURE_*)
//   bits 4-7    elevation structure kind
//   bits 8-11   on-elevation structure kind
//   bit 12      quantum field present
//   bits 16-23  building type (BUILDING_* or TOWER_*) at Elevation, or 0 if absent
//   bits 24-30  underground excavation shape (`VolumeShape::gpu_code`), or 0 if absent
@group(3) @binding(103) var<storage, read> obstacle_cells: array<u32>;

// Bit offsets of each volume's structure code in the packed cell.
const VOLUME_UNDERGROUND: u32 = 0u;
const VOLUME_ELEVATION: u32 = 4u;
const VOLUME_ON_ELEVATION: u32 = 8u;

// Mirrors `GridStructure::gpu_index` in indexes/src/obstacles.rs.
const STRUCTURE_EMPTY: u32 = 0u;
const STRUCTURE_SOLID: u32 = 1u;
const STRUCTURE_DARK_ORE: u32 = 2u;
const STRUCTURE_WALL: u32 = 3u;
const STRUCTURE_BUILDING: u32 = 4u;
const STRUCTURE_EXCAVATION: u32 = 5u;
// Structure code returned for queries outside the map; never stored in the buffer.
const STRUCTURE_OFF_MAP: u32 = 15u;

const STRUCTURE_MASK: u32 = 15u;
const QUANTUM_FIELD_BIT: u32 = 4096u;
const BUILDING_TYPE_SHIFT: u32 = 16u;
const UNDERGROUND_SHAPE_SHIFT: u32 = 24u;
const UNDERGROUND_SHAPE_MASK: u32 = 127u;
// Packed sentinel with STRUCTURE_OFF_MAP in all three volumes.
const OFF_MAP_CELL: u32 = 0x0FFFu;

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
const TOWER_CRUCIBLE: u32 = 40u;
const TOWER_NAPALM_BATTERY: u32 = 41u;

// Returns the packed cell, or OFF_MAP_CELL outside the map.
fn obstacle_cell_at(coords: vec2<i32>) -> u32 {
    if !grid_contains(coords) {
        return OFF_MAP_CELL;
    }
    return obstacle_cells[grid_index(coords)];
}

// Returns the structure code at `coords`; `volume` is a VOLUME_* bit offset.
fn structure_at(coords: vec2<i32>, volume: u32) -> u32 {
    return (obstacle_cell_at(coords) >> volume) & STRUCTURE_MASK;
}

fn building_type_at(coords: vec2<i32>) -> u32 {
    return (obstacle_cell_at(coords) >> BUILDING_TYPE_SHIFT) & 0xFFu;
}

// Extracts the underground shape bits, encoded by `VolumeShape::gpu_code`.
// Decode with `shape_ramp` and `shape_openings` from `shaders::walls::styles`.
fn underground_shape(cell: u32) -> u32 {
    return (cell >> UNDERGROUND_SHAPE_SHIFT) & UNDERGROUND_SHAPE_MASK;
}

fn is_tower_at(coords: vec2<i32>) -> bool {
    return building_type_at(coords) >= BUILDING_TOWER_FIRST;
}

fn is_within_quantum_field_at(coords: vec2<i32>) -> bool {
    return (obstacle_cell_at(coords) & QUANTUM_FIELD_BIT) != 0u;
}
