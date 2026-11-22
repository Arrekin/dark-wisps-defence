// Obstacle cells uploaded by indexes_internal through the shared GPU group.
// Custom materials enable this group through visuals::shared_gpu; lane shaders include it.

import package::shaders::common::map_info::{grid_contains, grid_index};

// One cell, row major. The layout mirrors `pack_obstacle_cell` in
// indexes_internal/src/obstacles.rs.
struct ObstacleCell {
    // bits 0-3    underground structure kind (STRUCTURE_*)
    // bits 4-7    on-ground structure kind
    // bits 8-11   on-elevation structure kind
    // bit 12      quantum field present
    // bits 16-23  building type (BUILDING_* or TOWER_*) at OnGround, or 0 if absent
    // bits 24-31  `WallStyleKey::gpu_index` of the wall at OnGround, or 0 if absent
    structures: u32,
    // Packed `StructureShape::gpu_code` (1 byte per level at bit offset `level * SHAPE_BITS`) for
    // walls and excavations, or 0 for other structures. Decoded by `shaders::grids::structure_shape`.
    shapes: u32,
};
@group(3) @binding(103) var<storage, read> obstacle_cells: array<ObstacleCell>;

// Level indices matching `AltitudeLevel::three_layer_index` in `game_core/src/altitude.rs`.
// A level's structure kind sits at bit offset `level * STRUCTURE_BITS` in `structures`, and its
// shape code at `level * SHAPE_BITS` in `shapes`.
const LEVEL_UNDERGROUND: u32 = 0u;
const LEVEL_ON_GROUND: u32 = 1u;
const LEVEL_ON_ELEVATION: u32 = 2u;
// Sentinel level for off-map rendering (such as UI faces); not valid for grid indexing.
const LEVEL_NONE: u32 = 0xFFFFFFFFu;

// Mirrors `GridStructure::gpu_index` in indexes/src/obstacles.rs.
const STRUCTURE_EMPTY: u32 = 0u;
const STRUCTURE_SOLID: u32 = 1u;
const STRUCTURE_DARK_ORE: u32 = 2u;
const STRUCTURE_WALL: u32 = 3u;
const STRUCTURE_BUILDING: u32 = 4u;
const STRUCTURE_EXCAVATION: u32 = 5u;
// Structure code returned for queries outside the map; never stored in the buffer.
const STRUCTURE_OFF_MAP: u32 = 15u;

const STRUCTURE_BITS: u32 = 4u;
const STRUCTURE_MASK: u32 = 15u;
const QUANTUM_FIELD_BIT: u32 = 4096u;
const BUILDING_TYPE_SHIFT: u32 = 16u;
const WALL_STYLE_SHIFT: u32 = 24u;
const SHAPE_BITS: u32 = 8u;
const SHAPE_MASK: u32 = 0x7Fu;
// Sentinel with STRUCTURE_OFF_MAP at all three levels and no shapes.
const OFF_MAP_CELL: ObstacleCell = ObstacleCell(0x0FFFu, 0u);

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

// Returns the cell, or OFF_MAP_CELL outside the map.
fn obstacle_cell_at(coords: vec2<i32>) -> ObstacleCell {
    if !grid_contains(coords) {
        return OFF_MAP_CELL;
    }
    return obstacle_cells[grid_index(coords)];
}

// Unpacks `cell`'s structure kind (`STRUCTURE_*`) at `level` (`LEVEL_*`).
fn cell_structure(cell: ObstacleCell, level: u32) -> u32 {
    return (cell.structures >> (level * STRUCTURE_BITS)) & STRUCTURE_MASK;
}

// Unpacks `cell`'s `StructureShape::gpu_code` at `level` (`LEVEL_*`).
fn cell_structure_shape(cell: ObstacleCell, level: u32) -> u32 {
    return (cell.shapes >> (level * SHAPE_BITS)) & SHAPE_MASK;
}

// Returns the structure code at `coords`; `level` is a LEVEL_* code.
fn structure_at(coords: vec2<i32>, level: u32) -> u32 {
    return cell_structure(obstacle_cell_at(coords), level);
}

// Returns the `StructureShape::gpu_code` at `coords` and `level` (`LEVEL_*`), or 0 if unshaped.
fn structure_shape_at(coords: vec2<i32>, level: u32) -> u32 {
    return cell_structure_shape(obstacle_cell_at(coords), level);
}

// True if `structure` (`STRUCTURE_*`) provides a floor for the level above it. Mirrors
// `GridStructure::provides_floor` in indexes/src/obstacles.rs.
fn structure_provides_floor(structure: u32) -> bool {
    return structure == STRUCTURE_SOLID || structure == STRUCTURE_DARK_ORE || structure == STRUCTURE_WALL
        || structure == STRUCTURE_EXCAVATION;
}

// Structure (`STRUCTURE_*`) in the layer below `level`. Solid earth lies below the lowest level.
// Mirrors `ObstacleCell::structure_under` in indexes/src/obstacles.rs.
fn cell_structure_under(cell: ObstacleCell, level: u32) -> u32 {
    if level == LEVEL_UNDERGROUND { return STRUCTURE_SOLID; }
    return cell_structure(cell, level - 1u);
}

// True if `level` has a floor in `cell`, provided by the structure beneath it. Mirrors
// `ObstacleCell::has_floor` in indexes/src/obstacles.rs.
fn cell_has_floor(cell: ObstacleCell, level: u32) -> bool {
    return structure_provides_floor(cell_structure_under(cell, level));
}

// The highest level of `cell` with a floor (`LEVEL_*`). Mirrors `ObstacleCell::top_level` in
// indexes/src/obstacles.rs: a building provides no floor, so its cell's top is the ground under it.
// OFF_MAP_CELL has no floor and returns LEVEL_UNDERGROUND.
fn cell_top_level(cell: ObstacleCell) -> u32 {
    if cell_has_floor(cell, LEVEL_ON_ELEVATION) { return LEVEL_ON_ELEVATION; }
    if cell_has_floor(cell, LEVEL_ON_GROUND) { return LEVEL_ON_GROUND; }
    return LEVEL_UNDERGROUND;
}

// The top level (`LEVEL_*`) of the cell at `coords`, as `cell_top_level` gives it.
fn top_level_at(coords: vec2<i32>) -> u32 {
    return cell_top_level(obstacle_cell_at(coords));
}

// Unpacks `cell`'s one-based wall style index (`WallStyleKey::gpu_index` in
// `map_objects/src/wall_style.rs`) at `LEVEL_ON_GROUND`, or 0 if `cell` has no wall.
fn cell_wall_style_index(cell: ObstacleCell) -> u32 {
    return cell.structures >> WALL_STYLE_SHIFT;
}

fn building_type_at(coords: vec2<i32>) -> u32 {
    return (obstacle_cell_at(coords).structures >> BUILDING_TYPE_SHIFT) & 0xFFu;
}

fn is_tower_at(coords: vec2<i32>) -> bool {
    return building_type_at(coords) >= BUILDING_TOWER_FIRST;
}

fn is_within_quantum_field_at(coords: vec2<i32>) -> bool {
    return (obstacle_cell_at(coords).structures & QUANTUM_FIELD_BIT) != 0u;
}
