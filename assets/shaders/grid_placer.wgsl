#import bevy_sprite::mesh2d_vertex_output::VertexOutput
#import dwd::core::{CELL_SIZE, grid_contains, grid_index}

// Draws placement validity around the footprint and highlights relevant cells. The object preview
// is rendered separately by a child entity below this quad.

// Cell states packed 2 bits per cell in cell_data. Mirror `build_cell_data` in
// grids_internal/src/placement.rs.
const CELL_OUTSIDE:  u32 = 0u;
const CELL_PLAIN:    u32 = 1u;
const CELL_NEGATIVE: u32 = 2u;
const CELL_POSITIVE: u32 = 3u;

// Mirror the `validity` mapping in `revalidate_placement`.
const VALIDITY_VALID:           u32 = 0u;
const VALIDITY_VALID_UNPOWERED: u32 = 1u;
const VALIDITY_INVALID:         u32 = 2u;

// Linear values for the sRGB hexes named beside them; the 2d pipeline renders HDR and tonemaps,
// so a colour written here is radiance, not a swatch.
const VALID_COLOR:     vec3<f32> = vec3<f32>(0.036, 0.479, 0.195);  // #35B87A
const UNPOWERED_COLOR: vec3<f32> = vec3<f32>(1.000, 0.558, 0.047);  // #FFC53D
const INVALID_COLOR:   vec3<f32> = vec3<f32>(1.000, 0.033, 0.033);  // #FF3333

// Invalid and positive cell marks use HDR values so bloom keeps them visible over the ghost.
// Valid footprint outlines stay within the display range.
const INVALID_EXPOSURE: f32 = 1.8;
const POSITIVE_EXPOSURE: f32 = 2.6;

// Stroke widths, stripe spacing and frame inset in world pixels.
const OUTLINE_WIDTH: f32 = 2.0;
const HATCH_SPACING: f32 = 10.0;
const HATCH_WIDTH_PLAIN: f32 = 1.5;
const HATCH_WIDTH_BLOCKING: f32 = 4.0;
const FRAME_INSET: f32 = 5.0;
const FRAME_WIDTH: f32 = 1.5;

// Opacity of the stripes over cells that are not themselves blocking.
const HATCH_DIM: f32 = 0.5;

const INV_SQRT_2: f32 = 0.70710678;

struct GridPlacerData {
    cell_data: vec4<u32>,  // 2 bits/cell, up to 64 cells (8×8 bounding box)
    cell_columns: u32,
    cell_rows: u32,
    validity: u32,
}

@group(2) @binding(0) var<uniform> data: GridPlacerData;

fn cell_state(cell: vec2<i32>) -> u32 {
    if !grid_contains(cell, data.cell_columns, data.cell_rows) {
        return CELL_OUTSIDE;
    }
    let cell_index = grid_index(cell, data.cell_columns);
    let bit_offset = (cell_index % 16u) * 2u;
    var word: u32;
    switch cell_index / 16u {
        case 0u:  { word = data.cell_data.x; }
        case 1u:  { word = data.cell_data.y; }
        case 2u:  { word = data.cell_data.z; }
        default:  { word = data.cell_data.w; }
    }
    return (word >> bit_offset) & 3u;
}

fn is_covered(cell: vec2<i32>) -> bool {
    return cell_state(cell) != CELL_OUTSIDE;
}

fn validity_color() -> vec3<f32> {
    if data.validity == VALIDITY_INVALID { return INVALID_COLOR * INVALID_EXPOSURE; }
    if data.validity == VALIDITY_VALID_UNPOWERED { return UNPOWERED_COLOR; }
    return VALID_COLOR;
}

// The boundary turns at a cell corner when both sides of the corner are covered and the cell
// diagonally across it is not, so the nearest point on the boundary is that corner itself.
// `offsets` holds the distances to the corner's two cell edges.
fn corner_distance(cell: vec2<i32>, direction: vec2<i32>, offsets: vec2<f32>) -> f32 {
    let diagonal_open = !is_covered(cell + direction);
    let sides_closed = is_covered(cell + vec2<i32>(direction.x, 0)) && is_covered(cell + vec2<i32>(0, direction.y));
    if diagonal_open && sides_closed {
        return length(offsets);
    }
    return CELL_SIZE;
}

// Distance in world pixels to the nearest edge of the covered footprint, which follows the
// imprint's real shape rather than its bounding box. Meaningful only inside a covered cell.
fn footprint_edge_distance(world: vec2<f32>, cell: vec2<i32>) -> f32 {
    let to_low = world - vec2<f32>(cell) * CELL_SIZE;
    let to_high = vec2<f32>(CELL_SIZE) - to_low;

    var nearest_edge = CELL_SIZE;
    if !is_covered(cell + vec2<i32>(-1,  0)) { nearest_edge = min(nearest_edge, to_low.x); }
    if !is_covered(cell + vec2<i32>( 1,  0)) { nearest_edge = min(nearest_edge, to_high.x); }
    if !is_covered(cell + vec2<i32>( 0, -1)) { nearest_edge = min(nearest_edge, to_low.y); }
    if !is_covered(cell + vec2<i32>( 0,  1)) { nearest_edge = min(nearest_edge, to_high.y); }

    nearest_edge = min(nearest_edge, corner_distance(cell, vec2<i32>(-1, -1), vec2<f32>(to_low.x,  to_low.y)));
    nearest_edge = min(nearest_edge, corner_distance(cell, vec2<i32>( 1, -1), vec2<f32>(to_high.x, to_low.y)));
    nearest_edge = min(nearest_edge, corner_distance(cell, vec2<i32>(-1,  1), vec2<f32>(to_low.x,  to_high.y)));
    nearest_edge = min(nearest_edge, corner_distance(cell, vec2<i32>( 1,  1), vec2<f32>(to_high.x, to_high.y)));
    return nearest_edge;
}

// Parallel 45° stripes measured along their own normal, so they run unbroken across the whole
// footprint instead of restarting at each cell border.
fn hatch_coverage(world: vec2<f32>, half_width: f32, texel: f32) -> f32 {
    let along_normal = (world.x + world.y) * INV_SQRT_2;
    let phase = fract(along_normal / HATCH_SPACING);
    let to_stripe_center = min(phase, 1.0 - phase) * HATCH_SPACING;
    return 1.0 - smoothstep(half_width - texel, half_width + texel, to_stripe_center);
}

// A square outline set in from the cell's borders, so a marked cell reads without covering the
// object drawn underneath it.
fn cell_frame_coverage(world: vec2<f32>, cell: vec2<i32>, texel: f32) -> f32 {
    let from_center = abs(world - (vec2<f32>(cell) + 0.5) * CELL_SIZE);
    let box_distance = max(from_center.x, from_center.y) - (CELL_SIZE * 0.5 - FRAME_INSET);
    return 1.0 - smoothstep(FRAME_WIDTH * 0.5 - texel, FRAME_WIDTH * 0.5 + texel, abs(box_distance));
}

@fragment
fn fragment(mesh: VertexOutput) -> @location(0) vec4<f32> {
    let grid_size = vec2<f32>(f32(data.cell_columns), f32(data.cell_rows));
    // Mesh UV has y running down and the cell grid has it running up.
    let world = vec2<f32>(mesh.uv.x, 1.0 - mesh.uv.y) * grid_size * CELL_SIZE;
    let cell = clamp(vec2<i32>(floor(world / CELL_SIZE)), vec2<i32>(0), vec2<i32>(grid_size) - 1);

    // World-coordinate span of one screen pixel at the quad's rendered size; evaluated before
    // divergent control flow.
    let texel = max(fwidth(world.x), 0.001);

    let state = cell_state(cell);
    if state == CELL_OUTSIDE {
        return vec4<f32>(0.0);
    }

    let outline_color = validity_color();
    var mark_color = outline_color;
    var mark_coverage = 0.0;
    if data.validity == VALIDITY_INVALID {
        let blocking = state == CELL_NEGATIVE;
        let half_width = select(HATCH_WIDTH_PLAIN, HATCH_WIDTH_BLOCKING, blocking) * 0.5;
        mark_coverage = hatch_coverage(world, half_width, texel) * select(HATCH_DIM, 1.0, blocking);
    } else if state == CELL_POSITIVE {
        mark_coverage = cell_frame_coverage(world, cell, texel);
        mark_color = outline_color * POSITIVE_EXPOSURE;
    }

    let edge_distance = footprint_edge_distance(world, cell);
    let outline_coverage = 1.0 - smoothstep(OUTLINE_WIDTH - texel, OUTLINE_WIDTH + texel, edge_distance);

    // Cell frames are inset far enough not to overlap the footprint outline.
    let color = select(mark_color, outline_color, outline_coverage > mark_coverage);
    return vec4<f32>(color, max(mark_coverage, outline_coverage));
}
