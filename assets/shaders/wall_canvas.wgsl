#import bevy_sprite::mesh2d_vertex_output::VertexOutput
#import dwd::wall_style::{WallStyle, LIGHT_PROBE, eroded_distance, plate_noise, wall_shading}

// Wall canvas
//
// Draws every wall on the map in one pass over a quad covering the whole grid. There is no
// tileset and no per-cell bitmask: the shader reads the eight neighbours of the cell a pixel
// falls in and builds a signed distance to the wall region from them. Every layer is a
// function of that one distance, and the layers themselves live in `dwd::wall_style` so the
// map canvas and a UI swatch cannot drift apart.
//
// Each of the eight neighbours whose style differs from ours contributes one candidate
// distance and the field is their minimum. A cardinal neighbour's nearest point is the shared
// cell edge; a diagonal neighbour's is the grid corner the two cells share.
//
//     \  |  /        Each neighbour is compared against this cell's own style, which lets one
//    ----+----       function serve both sides: inside a wall it measures to the nearest open
//     /  |  \        ground, on open ground it measures to the nearest wall.
//
// A neighbour of a different style counts as open ground, so two styles meet at a hard border.
//
// The field has square corners and carries no surface normal. The shading layers take their
// facing from the probe in `fragment`, which stays continuous where a candidate direction
// would not.

struct WallCanvasSettings {
    grid_width: u32,
    grid_height: u32,
    // Which term to draw instead of the finished wall. Mirrors `WallCanvasDebug` in
    // map_objects/src/wall_style.rs; the two must stay in step.
    debug_mode: u32,
}

// One style index per cell, row major, 0 for open ground.
@group(2) @binding(0) var<storage, read> cells: array<u32>;
// The map's style table. Cell value 1 is styles[0].
@group(2) @binding(1) var<storage, read> styles: array<WallStyle>;
@group(2) @binding(2) var<uniform> settings: WallCanvasSettings;

const CELL_SIZE: f32 = 32.0;
// Cell units. Nothing reads the field deeper than the bevel, so the interior is flat.
const DISTANCE_CAP: f32 = 0.5;
const SHADOW_STRENGTH: f32 = 0.85;

// Diagnostics. Each replaces the output with a single term, so an artefact can be attributed
// to one layer. Driven by `WallCanvasDebug` in map_objects/src/wall_style.rs; these values are
// that enum's discriminants and must stay in step with it.
const DEBUG_OFF: u32 = 0u;
const DEBUG_DISTANCE: u32 = 1u;
const DEBUG_FACING: u32 = 2u;
const DEBUG_NOISE: u32 = 3u;

fn cell_style(coords: vec2<i32>) -> u32 {
    // Out of bounds reads as open ground, so map borders get an edge like any other.
    if coords.x < 0 || coords.y < 0 || coords.x >= i32(settings.grid_width) || coords.y >= i32(settings.grid_height) {
        return 0u;
    }
    return cells[u32(coords.y) * settings.grid_width + u32(coords.x)];
}

struct WallSurface {
    // Cell units, positive inside a wall.
    distance: f32,
    // Style of the nearest wall. Only meaningful when the pixel is on open ground.
    style: u32,
}

fn closer(a: WallSurface, b: WallSurface) -> WallSurface {
    if b.distance < a.distance { return b; }
    return a;
}

// `p` is the position inside the cell, in [-0.5, 0.5] on both axes.
fn wall_surface(coords: vec2<i32>, p: vec2<f32>) -> WallSurface {
    let region = cell_style(coords);

    // Distance from p to each of the four cell edges.
    let toward_positive = vec2<f32>(0.5) - p;
    let toward_negative = vec2<f32>(0.5) + p;

    var cardinals = array<vec2<i32>, 4>(vec2<i32>(1, 0), vec2<i32>(-1, 0), vec2<i32>(0, 1), vec2<i32>(0, -1));
    var diagonals = array<vec2<i32>, 4>(vec2<i32>(1, 1), vec2<i32>(-1, 1), vec2<i32>(1, -1), vec2<i32>(-1, -1));

    var best = WallSurface(DISTANCE_CAP, 0u);

    for (var i = 0; i < 4; i++) {
        let offset = cardinals[i];
        let neighbour = cell_style(coords + offset);
        if neighbour == region { continue; }
        let edges = select(toward_negative, toward_positive, offset > vec2<i32>(0));
        let edge = select(edges.y, edges.x, offset.x != 0);
        best = closer(best, WallSurface(edge, neighbour));
    }

    for (var i = 0; i < 4; i++) {
        let offset = diagonals[i];
        let neighbour = cell_style(coords + offset);
        if neighbour == region { continue; }
        // Nearest point of a diagonal neighbour is the grid corner the two cells share.
        let edge = select(toward_negative, toward_positive, offset > vec2<i32>(0));
        best = closer(best, WallSurface(max(length(edge), 1e-5), neighbour));
    }

    return WallSurface(best.distance * select(-1.0, 1.0, region != 0u), best.style);
}

// Signed distance in world pixels at an arbitrary world position.
fn distance_at(world: vec2<f32>) -> f32 {
    let f = world / CELL_SIZE;
    let base = floor(f);
    return wall_surface(vec2<i32>(base), f - base - vec2<f32>(0.5)).distance * CELL_SIZE;
}

@fragment
fn fragment(mesh: VertexOutput) -> @location(0) vec4<f32> {
    let grid_size = vec2<f32>(f32(settings.grid_width), f32(settings.grid_height));
    let f = mesh.uv * grid_size;
    let base = floor(f);
    let coords = vec2<i32>(base);
    let world = f * CELL_SIZE;

    // World pixels covered by one screen pixel. Derivatives must be taken in uniform control
    // flow, so this stays above every early return. It is derived from the position, so the
    // erosion noise added to `d` cannot feed back into the antialiasing width.
    let texel = max(max(fwidth(world.x), fwidth(world.y)), 0.0001);

    let region = cell_style(coords);
    let surface = wall_surface(coords, f - base - vec2<f32>(0.5));

    // Inside a wall the pixel belongs to its own style; on open ground it belongs to
    // whichever wall is nearest, which may be one of several bordering that cell.
    let style_index = select(surface.style, region, region != 0u);
    if style_index == 0u { return vec4<f32>(0.0); }
    let style = styles[style_index - 1u];

    let raw_distance = surface.distance * CELL_SIZE;
    let d = eroded_distance(raw_distance, world, style);

    let coverage = smoothstep(-texel, texel, d);

    // Contact shadow, sampled at an offset along the light so the slab reads as lifted.
    let shadow_offset = style.surface.light_direction * style.surface.shadow_length * 0.7;
    let shadow_distance = distance_at(world - shadow_offset);
    let shadow = pow(saturate(1.0 + shadow_distance / max(style.surface.shadow_length, 0.0001)), 1.5)
        * SHADOW_STRENGTH * (1.0 - coverage);

    let alpha = coverage + shadow * (1.0 - coverage);
    if alpha < 0.002 { return vec4<f32>(0.0); }

    // How much the field falls away along the light: dot(outward normal, light) wherever the
    // field is smooth, and an average of the two faces across a mitre. Probing at least one
    // screen pixel keeps it from aliasing when zoomed out. Taken from the uneroded distance, so
    // the grain does not shake the light.
    let probe = max(LIGHT_PROBE, texel);
    let lit = -(distance_at(world + style.surface.light_direction * probe) - raw_distance) / probe;

    let plate = plate_noise(world, style);
    let colour = wall_shading(d, lit, plate, texel, style);

    if settings.debug_mode != DEBUG_OFF {
        // Diagnostics draw the wall only, so the shape stays readable against the map.
        if coverage < 0.5 { return vec4<f32>(0.0); }
        switch settings.debug_mode {
            // Banded every 4 world pixels: a kink in the field shows as a bent band.
            case DEBUG_DISTANCE: { return vec4<f32>(vec3<f32>(fract(d / 4.0)), 1.0); }
            case DEBUG_FACING: { return vec4<f32>(saturate(-lit), saturate(lit), 0.0, 1.0); }
            case DEBUG_NOISE: { return vec4<f32>(vec3<f32>(saturate(plate)), 1.0); }
            default: {}
        }
    }

    // The shadow contributes black at `shadow` alpha, the wall contributes `colour` at
    // `coverage`. Both fold into one non-premultiplied result.
    return vec4<f32>(colour * coverage / alpha, alpha);
}
