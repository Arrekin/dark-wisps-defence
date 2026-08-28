#import bevy_sprite::mesh2d_vertex_output::VertexOutput
#import dwd::core::CELL_SIZE
#import dwd::dark_ore_crystals::{dark_ore_shading, ore_inset}

// Draws all dark ore in one pass over a quad covering the map grid. Each storage-buffer value is
// both an occupancy mask (`fill > 0`) and a normalized ore amount.
//
// Occupied fragments derive distance to open ground from the eight neighboring cells. Shared edges
// between occupied cells disappear, while a non-negative world-space inset gives outer boundaries
// an organic shape without drawing into empty cells.
//
// Fill is interpolated between occupied cell centers and renormalized by occupancy, preventing both
// visible cell seams and dilution from empty neighbors.

// Field order and types mirror `DarkOreCanvasSettings` in dark_ore_canvas.rs.
struct DarkOreCanvasSettings {
    grid_width: u32,
    grid_height: u32,
}

// One f32 per cell, row major — the normalised fill level, 0.0 meaning no ore.
@group(2) @binding(0) var<storage, read> cells: array<f32>;
@group(2) @binding(1) var<uniform> settings: DarkOreCanvasSettings;

// Maximum distance in cell coordinates (0.5 is half a cell width). Deeper interior is flat.
const DISTANCE_CAP: f32 = 0.5;

// Outer-corner radius in cell units; 0.5 makes an isolated cell circular.
const CORNER_ROUND: f32 = 0.45;

fn cell_fill(coords: vec2<i32>) -> f32 {
    // Out of bounds reads as open ground, so map borders get an edge like any other.
    if coords.x < 0 || coords.y < 0 || coords.x >= i32(settings.grid_width) || coords.y >= i32(settings.grid_height) {
        return 0.0;
    }
    return cells[u32(coords.y) * settings.grid_width + u32(coords.x)];
}

fn is_dark_ore(coords: vec2<i32>) -> bool {
    return cell_fill(coords) > 0.0;
}

// Distance to open ground in cell units. `coords` is occupied and `p` is the position inside it,
// in [-0.5, 0.5] on both axes.
fn dark_ore_distance(coords: vec2<i32>, p: vec2<f32>) -> f32 {
    let toward_positive = vec2<f32>(0.5) - p;
    let toward_negative = vec2<f32>(0.5) + p;

    let open_px = !is_dark_ore(coords + vec2<i32>(1, 0));
    let open_nx = !is_dark_ore(coords + vec2<i32>(-1, 0));
    let open_py = !is_dark_ore(coords + vec2<i32>(0, 1));
    let open_ny = !is_dark_ore(coords + vec2<i32>(0, -1));

    var best = DISTANCE_CAP;
    if open_px { best = min(best, toward_positive.x); }
    if open_nx { best = min(best, toward_negative.x); }
    if open_py { best = min(best, toward_positive.y); }
    if open_ny { best = min(best, toward_negative.y); }

    if !open_px && !open_py && !is_dark_ore(coords + vec2<i32>(1, 1)) {
        best = min(best, max(length(toward_positive), 1e-5));
    }
    if !open_nx && !open_py && !is_dark_ore(coords + vec2<i32>(-1, 1)) {
        best = min(best, max(length(vec2<f32>(toward_negative.x, toward_positive.y)), 1e-5));
    }
    if !open_px && !open_ny && !is_dark_ore(coords + vec2<i32>(1, -1)) {
        best = min(best, max(length(vec2<f32>(toward_positive.x, toward_negative.y)), 1e-5));
    }
    if !open_nx && !open_ny && !is_dark_ore(coords + vec2<i32>(-1, -1)) {
        best = min(best, max(length(toward_negative), 1e-5));
    }

    // Replace the minimum of two boundary-edge distances with an arc near exposed outer corners.
    if open_px && open_py { best = min(best, rounded_corner(toward_positive.x, toward_positive.y)); }
    if open_px && open_ny { best = min(best, rounded_corner(toward_positive.x, toward_negative.y)); }
    if open_nx && open_py { best = min(best, rounded_corner(toward_negative.x, toward_positive.y)); }
    if open_nx && open_ny { best = min(best, rounded_corner(toward_negative.x, toward_negative.y)); }

    return best;
}

// Continuous rounded-corner distance from the distances to its two incident cell edges.
fn rounded_corner(a: f32, b: f32) -> f32 {
    if a >= CORNER_ROUND || b >= CORNER_ROUND {
        return min(a, b);
    }
    return CORNER_ROUND - length(vec2<f32>(CORNER_ROUND - a, CORNER_ROUND - b));
}

// Mask-weighted bilinear interpolation between cell centers. Empty cells contribute no weight, so
// boundary fill is not diluted while adjacent occupied cells still blend smoothly.
fn fill_at(f: vec2<f32>) -> f32 {
    // Shift half a cell so the four nearest cell centers bound the interpolation square.
    let shifted = f - vec2<f32>(0.5);
    let base = floor(shifted);
    let t = shifted - base;
    let corner = vec2<i32>(base);

    let weight_00 = (1.0 - t.x) * (1.0 - t.y);
    let weight_10 = t.x * (1.0 - t.y);
    let weight_01 = (1.0 - t.x) * t.y;
    let weight_11 = t.x * t.y;

    let fill_00 = cell_fill(corner);
    let fill_10 = cell_fill(corner + vec2<i32>(1, 0));
    let fill_01 = cell_fill(corner + vec2<i32>(0, 1));
    let fill_11 = cell_fill(corner + vec2<i32>(1, 1));

    var total = 0.0;
    var weight = 0.0;
    if fill_00 > 0.0 { total = total + weight_00 * fill_00; weight = weight + weight_00; }
    if fill_10 > 0.0 { total = total + weight_10 * fill_10; weight = weight + weight_10; }
    if fill_01 > 0.0 { total = total + weight_01 * fill_01; weight = weight + weight_01; }
    if fill_11 > 0.0 { total = total + weight_11 * fill_11; weight = weight + weight_11; }

    if weight <= 0.0 { return 0.0; }
    return total / weight;
}

@fragment
fn fragment(mesh: VertexOutput) -> @location(0) vec4<f32> {
    let grid_size = vec2<f32>(f32(settings.grid_width), f32(settings.grid_height));
    let f = mesh.uv * grid_size;
    let world = f * CELL_SIZE;

    // World-coordinate span of one screen pixel; evaluated before divergent control flow.
    let texel = max(max(fwidth(world.x), fwidth(world.y)), 0.0001);

    let base = floor(f);
    let coords = vec2<i32>(base);

    // Every visible layer fades out inside the logical ore region, so empty cells have nothing to
    // draw and never pay for the noise or crystal field.
    if !is_dark_ore(coords) { return vec4<f32>(0.0); }

    let d = dark_ore_distance(coords, f - base - vec2<f32>(0.5)) * CELL_SIZE - ore_inset(world);
    if d <= 0.0 { return vec4<f32>(0.0); }

    return dark_ore_shading(d, fill_at(f), texel, world);
}
