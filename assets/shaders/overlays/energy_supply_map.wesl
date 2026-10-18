import bevy_sprite_render::mesh2d::vertex_output::VertexOutput;
import package::shaders::common::map_info::{grid_bounds, grid_contains, grid_index};

// Energy Supply Overlay Shader
//
// Layers (back to front):
//  1. Fill: yellow (powered) / orange (unpowered) tint over cells with active supply.
//     Disabled coverage contributes NO fill.
//  2. Supply edges: solid outlines where supply presence or highlight state changes
//     between neighboring cells. Drawn double-sided (both cells along the boundary).
//  3. Disabled edges: red dashed outlines around the union of ranges of intentionally
//     disabled suppliers. Drawn single-sided (from inside the disabled region).
//     When a disabled supplier is highlighted, its own range boundary inside the
//     union is drawn too (bright), since its cells rank above merely-covered cells.
//
// Edge priority on shared pixels (see edge_score): brighter (highlighted) edges beat
// dimmed ones; on equal brightness the disabled (red) edge wins, so intentional
// shutdowns stay visible on top of supply boundaries.
//
// Dash pattern (disabled edges only): two gaps per cell edge at 1/3 and 2/3 along it.
// Endpoints stay ON so corners join cleanly across cells.

// Field order and types mirror `EnergySupplyCell` in overlays/src/energy_supply.rs.
struct EnergySupplyCell {
    has_supply: u32,
    has_power: u32,
    highlight_level: u32,          // 0 = none, 1 = dimmed, 2 = highlighted
    has_disabled: u32,             // 1 if any disabled supplier covers this cell
    disabled_highlight_level: u32, // 0 = none, 1 = dimmed, 2 = highlighted
}

@group(2) @binding(0) var<storage, read> energy_cells: array<EnergySupplyCell>;

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------
const BLOCK_SIZE: f32 = 32.; // Size of each block in pixels
const SUPPLY_OUTLINE_THICKNESS: f32 = 2.; // Supply outline size in pixels
// Keep disabled coverage visible when zoomed out with a thicker red dashed outline.
const DISABLED_OUTLINE_THICKNESS: f32 = 4.; // Pixels
const SUPPLY_OUTLINE_RATIO: f32 = SUPPLY_OUTLINE_THICKNESS / BLOCK_SIZE;
const DISABLED_OUTLINE_RATIO: f32 = DISABLED_OUTLINE_THICKNESS / BLOCK_SIZE;

// Half-width of each dash gap, in [0..1] of the cell edge (disabled edges only)
const GAP_HALF: f32 = 0.09;

const BASE_COLOR: vec4<f32> = vec4<f32>(1., 1., 1., 0.); // Transparent
const HAS_POWER_COLOR: vec4<f32> = vec4<f32>(1., 1., 0., 0.5); // Yellow
const NO_POWER_COLOR: vec4<f32> = vec4<f32>(1., 0.2, 0., 0.5); // Orange
const DISABLED_COLOR: vec4<f32> = vec4<f32>(1., 0., 0.05, 0.5); // Red

const FILL_ALPHA_DIMMED: f32 = 5.0 / 255.0;
const FILL_ALPHA_HIGHLIGHTED: f32 = 15.0 / 255.0;
const EDGE_ALPHA_DIMMED: f32 = 0.2;
const EDGE_ALPHA_HIGHLIGHTED: f32 = 0.9;

// Edge kinds
const EDGE_NONE: u32 = 0u;
const EDGE_SUPPLY: u32 = 1u;
const EDGE_DISABLED: u32 = 2u;

// ---------------------------------------------------------------------------
// Cell sampling
// ---------------------------------------------------------------------------
fn get_cell_data(uv: vec2<f32>) -> EnergySupplyCell {
    // Clamp UV coordinates to valid range to prevent bleeding at edges
    let clamped_uv = clamp(uv, vec2<f32>(0.0), vec2<f32>(0.9999));
    let coords = vec2<i32>(clamped_uv * vec2<f32>(grid_bounds()));
    let index = grid_index(coords);

    if (!grid_contains(coords) || index >= arrayLength(&energy_cells)) {
        return EnergySupplyCell(0u, 0u, 0u, 0u, 0u);
    }

    return energy_cells[index];
}

// ---------------------------------------------------------------------------
// Layer 1: fill
// ---------------------------------------------------------------------------
fn fill_color(cell: EnergySupplyCell) -> vec4<f32> {
    if (cell.has_supply == 0u) {
        return BASE_COLOR;
    }
    var color = select(NO_POWER_COLOR, HAS_POWER_COLOR, cell.has_power != 0u);
    if (cell.highlight_level == 0u) {
        color.a = 0.0; // Transparent (no highlight)
    } else if (cell.highlight_level == 1u) {
        color.a = FILL_ALPHA_DIMMED;
    } else {
        color.a = FILL_ALPHA_HIGHLIGHTED;
    }
    return color;
}

// ---------------------------------------------------------------------------
// Layers 2+3: edges
// ---------------------------------------------------------------------------
struct EdgeCandidate {
    kind: u32,            // EDGE_NONE / EDGE_SUPPLY / EDGE_DISABLED
    highlighted: bool,    // bright vs dimmed outline
    power_boundary: bool, // supply edges only: has_power differs across the edge
}

// Priority for compositing overlapping edges. Brightness dominates; the disabled
// (red) edge wins ties so intentional shutdowns are never masked by supply outlines.
fn edge_score(edge: EdgeCandidate) -> u32 {
    if (edge.kind == EDGE_NONE) { return 0u; }
    let level = select(1u, 2u, edge.highlighted);
    return level * 2u + select(0u, 1u, edge.kind == EDGE_DISABLED);
}

fn better_edge(a: EdgeCandidate, b: EdgeCandidate) -> EdgeCandidate {
    if (edge_score(b) > edge_score(a)) { return b; }
    return a;
}

// Rank of a cell's disabled coverage; disabled edges are drawn from the higher-rank
// side. This yields both the union outline (1 vs 0) and, when a disabled supplier is
// highlighted, its own outline inside the union (2 vs 1).
fn disabled_rank(cell: EnergySupplyCell) -> u32 {
    if (cell.has_disabled == 0u) { return 0u; }
    return select(1u, 2u, cell.disabled_highlight_level == 2u);
}

// Static per-cell dash pattern: 2 evenly spaced gaps at 1/3 and 2/3 along the edge.
// `along` is the block-position coordinate running parallel to the edge.
fn dashed_mask(along: f32) -> bool {
    let gap1 = abs(along - (1.0 / 3.0)) < GAP_HALF;
    let gap2 = abs(along - (2.0 / 3.0)) < GAP_HALF;
    return !(gap1 || gap2);
}

// Best edge produced by the boundary between `cell` and one neighbor.
// `supply_on`: pixel is within the (narrower) supply outline band.
// `dash_on` is the dash mask for the disabled edge (supply edges are solid).
fn edge_candidate(cell: EnergySupplyCell, neighbor: EnergySupplyCell, supply_on: bool, dash_on: bool) -> EdgeCandidate {
    var best = EdgeCandidate(EDGE_NONE, false, false);

    // Supply edge: drawn on supply-presence or highlight-state boundaries (double-sided).
    let supply_boundary = cell.has_supply != neighbor.has_supply;
    let highlight_boundary = (cell.highlight_level == 2u) != (neighbor.highlight_level == 2u);
    if (supply_on && (supply_boundary || highlight_boundary)) {
        let power_boundary = cell.has_power != neighbor.has_power;
        best = EdgeCandidate(EDGE_SUPPLY, highlight_boundary, power_boundary);
    }

    // Disabled edge: dashed, drawn only from inside the disabled region.
    if (dash_on && disabled_rank(cell) > disabled_rank(neighbor)) {
        let candidate = EdgeCandidate(EDGE_DISABLED, cell.disabled_highlight_level == 2u, false);
        best = better_edge(best, candidate);
    }

    return best;
}

fn edge_color(edge: EdgeCandidate, cell: EnergySupplyCell) -> vec4<f32> {
    let alpha = select(EDGE_ALPHA_DIMMED, EDGE_ALPHA_HIGHLIGHTED, edge.highlighted);
    if (edge.kind == EDGE_DISABLED) {
        return vec4<f32>(DISABLED_COLOR.rgb, alpha);
    }
    // Power boundaries always use the powered color; otherwise the cell's own power state decides.
    let powered = edge.power_boundary || (cell.has_power != 0u);
    let color = select(NO_POWER_COLOR, HAS_POWER_COLOR, powered);
    return vec4<f32>(color.rgb, alpha);
}

// ---------------------------------------------------------------------------
// Fragment
// ---------------------------------------------------------------------------
@fragment
fn fragment(mesh: VertexOutput) -> @location(0) vec4<f32> {
    let uv = mesh.uv;
    let grid_size = vec2<f32>(grid_bounds());
    let block_position = fract(uv * grid_size);
    let step_size = 1.0 / grid_size;

    let cell = get_cell_data(uv);
    let fill = fill_color(cell);

    // Bands are computed at the wider (disabled) thickness; the narrower supply bands
    // gate supply edges inside edge_candidate.
    let in_right = block_position.x >= (1.0 - DISABLED_OUTLINE_RATIO);
    let in_top = block_position.y >= (1.0 - DISABLED_OUTLINE_RATIO);
    let in_vertical_band = (block_position.x <= DISABLED_OUTLINE_RATIO) || in_right;   // near a vertical cell edge
    let in_horizontal_band = (block_position.y <= DISABLED_OUTLINE_RATIO) || in_top;   // near a horizontal cell edge
    let in_supply_vertical_band = (block_position.x <= SUPPLY_OUTLINE_RATIO) || (block_position.x >= (1.0 - SUPPLY_OUTLINE_RATIO));
    let in_supply_horizontal_band = (block_position.y <= SUPPLY_OUTLINE_RATIO) || (block_position.y >= (1.0 - SUPPLY_OUTLINE_RATIO));

    if (!(in_vertical_band || in_horizontal_band)) {
        return fill;
    }

    var best = EdgeCandidate(EDGE_NONE, false, false);

    if (in_vertical_band) {
        // Vertical edge: sample left/right neighbor; dash runs along y.
        let neighbor_uv = uv + vec2<f32>(select(-step_size.x, step_size.x, in_right), 0.0);
        best = better_edge(best, edge_candidate(cell, get_cell_data(neighbor_uv), in_supply_vertical_band, dashed_mask(block_position.y)));
    }
    if (in_horizontal_band) {
        // Horizontal edge: sample bottom/top neighbor; dash runs along x.
        let neighbor_uv = uv + vec2<f32>(0.0, select(-step_size.y, step_size.y, in_top));
        best = better_edge(best, edge_candidate(cell, get_cell_data(neighbor_uv), in_supply_horizontal_band, dashed_mask(block_position.x)));
    }
    if (in_vertical_band && in_horizontal_band) {
        // Corner: also check the diagonal neighbor to fill inner/outer corner pixels
        // the axis checks miss. Corners are always dash-ON for clean joins.
        let neighbor_uv = uv + vec2<f32>(select(-step_size.x, step_size.x, in_right),
                                         select(-step_size.y, step_size.y, in_top));
        best = better_edge(best, edge_candidate(cell, get_cell_data(neighbor_uv), in_supply_vertical_band && in_supply_horizontal_band, true));
    }

    if (best.kind == EDGE_NONE) {
        return fill;
    }
    return edge_color(best, cell);
}
