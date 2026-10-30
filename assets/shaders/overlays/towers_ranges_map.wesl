import bevy_sprite_render::mesh2d::vertex_output::VertexOutput;
import package::shaders::common::core::{CELL_SIZE, TAU};
import package::shaders::common::map_info::{grid_bounds, grid_contains, grid_index};

// Tower range overlay.
//
// Cell ranges draw dashed borders where neighbouring coverage signatures differ.
// Circular ranges draw dashed arcs using the circle slots listed in each cell.
// A separate uniform supplies the selected or previewed range. Its inner and outer boundaries
// draw over a glow that fades toward the interior. Highlights also outline covered map borders.

// Field order and types must match `TowerRangeCell` in overlays/src/towers_range.rs.
struct TowerRangeCell {
    // XOR of the entity indexes of towers whose cell ranges cover this cell.
    signature: u32,
    // Number of cell ranges covering this cell.
    cover_count: u32,
    // Offset of this cell's first entry in `circle_indexes`.
    circle_start: u32,
    // Number of circle slot indexes stored for this cell.
    circle_count: u32,
}

// Circular range data in world units. Free slots and cell ranges use zeroed entries.
// Field order and types must match `TowerRangeCircle` in overlays/src/towers_range.rs.
struct TowerRangeCircle {
    center: vec2<f32>,
    min_radius: f32,
    max_radius: f32,
}

@group(2) @binding(0) var<storage, read> cells: array<TowerRangeCell>;
@group(2) @binding(1) var<storage, read> circles: array<TowerRangeCircle>;
// Circle slot indexes grouped by cell, addressed by each cell's start offset and count.
@group(2) @binding(2) var<storage, read> circle_indexes: array<u32>;

// Must match the `kind` values of `TowerRangeHighlight` in overlays/src/towers_range.rs.
const HIGHLIGHT_NONE: u32 = 0u;
const HIGHLIGHT_CELLS: u32 = 1u;
const HIGHLIGHT_CIRCLE: u32 = 2u;

// Field order and types must match `TowerRangeHighlight` in overlays/src/towers_range.rs.
struct TowerRangeHighlight {
    kind: u32,
    // 1 for a plus imprint, 0 for a rectangle.
    is_plus: u32,
    // Bottom-left cell of the imprint's bounding box.
    origin: vec2<i32>,
    // Rectangle dimensions in cells, or plus arm extents repeated in both components.
    size: vec2<i32>,
    // Centre of the range in world units.
    center: vec2<f32>,
    // Inclusive minimum distance in cells.
    min: f32,
    // Inclusive maximum distance in cells.
    max: f32,
}

@group(2) @binding(3) var<uniform> highlight: TowerRangeHighlight;

// Rendering constants
// Outline width in cells. Must match `OUTLINE_RATIO` in overlays/src/towers_range.rs.
const OUTLINE_RATIO: f32 = 2.0 / 16.0;
const ARC_WIDTH: f32 = OUTLINE_RATIO * CELL_SIZE; // World units

// Half-width of each dash gap, as a fraction of one cell edge.
const GAP_HALF: f32 = 0.070;

const BASE_COLOR: vec4<f32> = vec4<f32>(0.0, 0.0, 0.0, 0.0); // Transparent
const OUTLINE_COLOR: vec4<f32> = vec4<f32>(1.0, 1.0, 1.0, 0.03); // White, dim
const SELECTED_OUTLINE_COLOR: vec4<f32> = vec4<f32>(0.0, 1.0, 1.0, 0.9); // Cyan bright outline

// Glow inside the highlighted range, strongest at its edges.
const GLOW_COLOR: vec3<f32> = vec3<f32>(0.0, 1.0, 1.0); // Cyan
const GLOW_EDGE_ALPHA: f32 = 0.12; // Just inside an edge
const GLOW_BASE_ALPHA: f32 = 0.025; // Away from the edges
const GLOW_FADE_CELLS: f32 = 1.5; // Distance over which the glow fades from edge to base
// Search radius for cell range boundaries. Must cover `GLOW_FADE_CELLS`.
const GLOW_SEARCH_CELLS: i32 = 2;

fn get_cell_data(uv: vec2<f32>) -> TowerRangeCell {
    // Clamp UV coordinates to valid range to prevent bleeding at edges
    let clamped_uv = clamp(uv, vec2<f32>(0.0), vec2<f32>(0.9999));
    let coords = vec2<i32>(clamped_uv * vec2<f32>(grid_bounds()));
    let index = grid_index(coords);

    if index >= arrayLength(&cells) {
        return TowerRangeCell(0u, 0u, 0u, 0u);
    }

    return cells[index];
}

// Dash mask for a position along one cell edge, normalized to [0, 1].
// Gaps at 1/3 and 2/3 leave endpoints filled so adjacent edges join without breaks.
fn dash_on(along: f32) -> bool {
    let gap1 = abs(along - (1.0 / 3.0)) < GAP_HALF;
    let gap2 = abs(along - (2.0 / 3.0)) < GAP_HALF;
    return !(gap1 || gap2);
}

// Repeats the cell dash pattern around a ring, rounding the repeat count to close without a seam.
// Using the ring's radius keeps dashes aligned across the outline's width.
fn arc_dash_mask(to_pixel: vec2<f32>, radius: f32) -> bool {
    let cells_per_turn = round(TAU * radius / CELL_SIZE);
    return dash_on(fract(atan2(to_pixel.y, to_pixel.x) / TAU * cells_per_turn));
}

// Tests the dashed inner and outer ring outlines. Positions and radii use world units.
// Both outlines extend toward the covered region; a zero minimum has no inner outline.
fn on_ring_edges(world_position: vec2<f32>, center: vec2<f32>, min_radius: f32, max_radius: f32) -> bool {
    let to_pixel = world_position - center;
    let distance = length(to_pixel);
    let on_max = distance <= max_radius && distance >= max_radius - ARC_WIDTH;
    let on_min = min_radius > 0.0 && distance >= min_radius && distance <= min_radius + ARC_WIDTH;
    return (on_max && arc_dash_mask(to_pixel, max_radius)) || (on_min && arc_dash_mask(to_pixel, min_radius));
}

// Tests the pixel against the circular outlines listed in its cell.
fn on_circle_edge(cell: TowerRangeCell, world_position: vec2<f32>) -> bool {
    for (var entry = cell.circle_start; entry < cell.circle_start + cell.circle_count; entry++) {
        let circle = circles[circle_indexes[entry]];
        if on_ring_edges(world_position, circle.center, circle.min_radius, circle.max_radius) {
            return true;
        }
    }
    return false;
}

// Grid distance from `value` to [start, start + length - 1], or zero within that interval.
fn span_distance(value: i32, start: i32, length: i32) -> i32 {
    return max(max(start - value, value - (start + length - 1)), 0);
}

// Cardinal grid steps from `coords` to the nearest cell of the highlighted imprint.
// Must match `GridArea::distance_to` in game_core/src/grid.rs.
fn highlight_cell_distance(coords: vec2<i32>) -> i32 {
    if highlight.is_plus == 0u {
        return span_distance(coords.x, highlight.origin.x, highlight.size.x) + span_distance(coords.y, highlight.origin.y, highlight.size.y);
    }
    let extents = highlight.size.x;
    let side = 2 * extents + 1;
    let to_vertical_bar = abs(coords.x - (highlight.origin.x + extents)) + span_distance(coords.y, highlight.origin.y, side);
    let to_horizontal_bar = abs(coords.y - (highlight.origin.y + extents)) + span_distance(coords.x, highlight.origin.x, side);
    return min(to_vertical_bar, to_horizontal_bar);
}

// Tests cell range coverage, treating cells outside the map as uncovered.
// The caller must supply a highlight with cell range geometry.
fn highlight_covers_cell(coords: vec2<i32>) -> bool {
    if !grid_contains(coords) {
        return false;
    }
    let distance = f32(highlight_cell_distance(coords));
    return highlight.min <= distance && distance <= highlight.max;
}

// Tests whether a cell range highlight covers `uv`. Returns false outside the map.
fn highlight_covers(uv: vec2<f32>) -> bool {
    return highlight.kind == HIGHLIGHT_CELLS && highlight_covers_cell(vec2<i32>(floor(uv * vec2<f32>(grid_bounds()))));
}

// Tests the highlighted circle's ring outlines and any map border within its range.
// Map border outlines extend inward to stay visible on the map canvas.
fn on_highlight_circle_edge(cell_position: vec2<f32>, world_position: vec2<f32>) -> bool {
    if on_ring_edges(world_position, highlight.center, highlight.min * CELL_SIZE, highlight.max * CELL_SIZE) {
        return true;
    }
    let bounds = vec2<f32>(grid_bounds());
    let in_vertical_band = cell_position.x <= OUTLINE_RATIO || cell_position.x >= bounds.x - OUTLINE_RATIO;
    let in_horizontal_band = cell_position.y <= OUTLINE_RATIO || cell_position.y >= bounds.y - OUTLINE_RATIO;
    if !((in_vertical_band && dash_on(fract(cell_position.y))) || (in_horizontal_band && dash_on(fract(cell_position.x)))) {
        return false;
    }
    let distance = length(world_position - highlight.center);
    return highlight.min * CELL_SIZE <= distance && distance <= highlight.max * CELL_SIZE;
}

// Distance to a highlighted boundary, in cells, including the map border.
// Returns a negative value outside the highlight. For cell ranges, searches only far enough
// to calculate the glow falloff. `covers_cell` indicates coverage by a cell range highlight.
fn highlight_edge_distance(cell_position: vec2<f32>, covers_cell: bool) -> f32 {
    if highlight.kind == HIGHLIGHT_CIRCLE {
        let distance = length(cell_position * CELL_SIZE - highlight.center) / CELL_SIZE;
        let to_max = highlight.max - distance;
        let to_min = select(to_max, distance - highlight.min, highlight.min > 0.0);
        let bounds = vec2<f32>(grid_bounds());
        let to_border = min(min(cell_position.x, cell_position.y), min(bounds.x - cell_position.x, bounds.y - cell_position.y));
        return min(min(to_max, to_min), to_border);
    }
    if !covers_cell {
        return -1.0;
    }
    let coords = vec2<i32>(floor(cell_position));
    // Distance to the nearest uncovered cell gives the distance to the boundary.
    var nearest = f32(GLOW_SEARCH_CELLS + 1);
    for (var dy = -GLOW_SEARCH_CELLS; dy <= GLOW_SEARCH_CELLS; dy++) {
        for (var dx = -GLOW_SEARCH_CELLS; dx <= GLOW_SEARCH_CELLS; dx++) {
            let neighbor = coords + vec2<i32>(dx, dy);
            if highlight_covers_cell(neighbor) {
                continue;
            }
            let corner = vec2<f32>(neighbor);
            let outside = max(corner - cell_position, cell_position - (corner + 1.0));
            nearest = min(nearest, length(max(outside, vec2<f32>(0.0))));
        }
    }
    return nearest;
}

// Highlight glow, transparent outside the range.
// `covers_cell` indicates coverage by a cell range highlight.
fn highlight_glow(cell_position: vec2<f32>, covers_cell: bool) -> vec4<f32> {
    let edge_distance = highlight_edge_distance(cell_position, covers_cell);
    if edge_distance < 0.0 {
        return BASE_COLOR;
    }
    let near_edge = 1.0 - clamp(edge_distance / GLOW_FADE_CELLS, 0.0, 1.0);
    return vec4<f32>(GLOW_COLOR, mix(GLOW_BASE_ALPHA, GLOW_EDGE_ALPHA, near_edge * near_edge));
}

// Composites `top` over `bottom` using straight (non-premultiplied) alpha.
fn over(top: vec4<f32>, bottom: vec4<f32>) -> vec4<f32> {
    let alpha = top.a + bottom.a * (1.0 - top.a);
    if alpha <= 0.0 {
        return BASE_COLOR;
    }
    return vec4<f32>((top.rgb * top.a + bottom.rgb * bottom.a * (1.0 - top.a)) / alpha, alpha);
}

// Chooses which cell draws a shared border when coverage signatures differ.
// The cell with more coverage draws it. Tied counts above one use the larger signature;
// two cells each covered by one range both draw, preserving each range's outline.
fn draws_signature_edge(cell: TowerRangeCell, neighbor: TowerRangeCell) -> bool {
    return cell.cover_count > 0u && cell.signature != neighbor.signature
        && (cell.cover_count > neighbor.cover_count
            || (cell.cover_count == neighbor.cover_count && (cell.cover_count == 1u || cell.signature > neighbor.signature)));
}

@fragment
fn fragment(mesh: VertexOutput) -> @location(0) vec4<f32> {
    let uv = mesh.uv;
    let cell_position = uv * vec2<f32>(grid_bounds());
    let world_position = cell_position * CELL_SIZE;

    let position_in_cell = fract(cell_position);

    // Identify outline bands and which neighbouring cells they touch.
    let is_right_band = position_in_cell.x >= (1.0 - OUTLINE_RATIO);
    let is_top_band = position_in_cell.y >= (1.0 - OUTLINE_RATIO);
    let in_vertical_band = position_in_cell.x <= OUTLINE_RATIO || is_right_band;
    let in_horizontal_band = position_in_cell.y <= OUTLINE_RATIO || is_top_band;
    let at_edge = in_vertical_band || in_horizontal_band;

    let cell = get_cell_data(uv);
    let covers_cell = highlight_covers(uv);
    let glow = highlight_glow(cell_position, covers_cell);

    var draw_highlight_any = false;
    var draw_signature_any = false;
    // Cell range outlines draw from covered cells.
    if at_edge && (cell.cover_count > 0u || covers_cell) {
        let step_size = 1.0 / vec2<f32>(grid_bounds());
        let neighbor_step = vec2<f32>(select(-step_size.x, step_size.x, is_right_band), select(-step_size.y, step_size.y, is_top_band));

        var draw_highlight_v = false;
        var draw_signature_v = false;
        var dash_v = false;
        if in_vertical_band {
            let neighbor_uv_v = uv + vec2<f32>(neighbor_step.x, 0.0);
            draw_highlight_v = covers_cell && !highlight_covers(neighbor_uv_v);
            draw_signature_v = draws_signature_edge(cell, get_cell_data(neighbor_uv_v));
            dash_v = dash_on(position_in_cell.y);
        }

        var draw_highlight_h = false;
        var draw_signature_h = false;
        var dash_h = false;
        if in_horizontal_band {
            let neighbor_uv_h = uv + vec2<f32>(0.0, neighbor_step.y);
            draw_highlight_h = covers_cell && !highlight_covers(neighbor_uv_h);
            draw_signature_h = draws_signature_edge(cell, get_cell_data(neighbor_uv_h));
            dash_h = dash_on(position_in_cell.x);
        }

        // Highlight outlines take priority over coverage outlines.
        draw_highlight_any = (draw_highlight_v && dash_v) || (draw_highlight_h && dash_h);
        draw_signature_any = ((draw_signature_v && dash_v) || (draw_signature_h && dash_h)) && !draw_highlight_any;

        // Fill corners where coverage differs diagonally, even if neither side drew an outline.
        if in_vertical_band && in_horizontal_band && !(draw_highlight_any || draw_signature_any) {
            let neighbor_uv_d = uv + neighbor_step;
            let draw_highlight_d = covers_cell && !highlight_covers(neighbor_uv_d);
            let draw_signature_d = draws_signature_edge(cell, get_cell_data(neighbor_uv_d));
            if draw_highlight_d {
                draw_highlight_any = true;
            } else if draw_signature_d {
                draw_signature_any = true;
            }
        }
    }

    if draw_highlight_any || (highlight.kind == HIGHLIGHT_CIRCLE && on_highlight_circle_edge(cell_position, world_position)) {
        return over(SELECTED_OUTLINE_COLOR, glow);
    }
    if draw_signature_any || on_circle_edge(cell, world_position) {
        return over(OUTLINE_COLOR, glow);
    }
    return glow;
}
