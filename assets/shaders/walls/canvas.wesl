// Draws walls and ramps in one pass over a quad covering the map. Packed cell data from
// `shaders::walls::styles` supplies each wall's style and ramp direction. Cells outside the map
// count as open ground, giving walls at the map boundary visible edges.
//
// Neighboring cells define a signed distance field for surface edges. Surfaces connect where
// their heights and styles match; a ramp's low edge can also connect to open ground.
// Ramps taper toward their low edge, exposing ground on either side. Heights range from 0
// at ground level to 1 at a wall top.
//
// Open ground uses the nearest wall's style for contact shadows. Flat walls estimate edge normals
// from the distance field; ramps use explicit normals. `shaders::walls::look` shades the surfaces.

import bevy_sprite_render::mesh2d::vertex_output::VertexOutput;
import package::shaders::common::core::CELL_SIZE;
import package::shaders::common::map_info::grid_bounds;
import package::shaders::common::map_light::MAP_SUN_GROUND_DIRECTION;
import package::shaders::walls::look::{BASE_INSET, CREST_BREAK, LIGHT_PROBE, edge_layers_reach, eroded_distance, plate_noise, ramp_height, ramp_tone, ramp_up, wall_shading};
import package::shaders::walls::styles::{cell_ramp, cell_style, wall_cell_at, wall_style, wall_style_index_at};

// Field order and types mirror `WallCanvasSettings` in map_objects_internal/src/wall_canvas.rs.
struct WallCanvasSettings {
    debug_mode: u32, // `WallCanvasDebug` discriminant.
}

@group(2) @binding(0) var<uniform> settings: WallCanvasSettings;

// Maximum distance in cell coordinates (0.5 is half a cell width). Deeper interior is flat.
const DISTANCE_CAP: f32 = 0.5;
const SHADOW_STRENGTH: f32 = 0.85;
// Shadow sampling offset toward the sun, as a fraction of the style's shadow length.
const SHADOW_OFFSET: f32 = 0.7;
// Maximum shadow reach, as a multiple of the style's shadow length.
const SHADOW_REACH: f32 = 1.0 + SHADOW_OFFSET;

// Diagnostic output modes; values mirror the `WallCanvasDebug` discriminants in map_objects/src/wall_style.rs.
const DEBUG_OFF: u32 = 0u;
const DEBUG_DISTANCE: u32 = 1u;
const DEBUG_FACING: u32 = 2u;
const DEBUG_NOISE: u32 = 3u;

// Edge height categories. EDGE_NONE marks ramp sides whose height varies along the edge.
const EDGE_NONE: u32 = 0u;
const EDGE_GROUND: u32 = 1u;
const EDGE_WALL_TOP: u32 = 2u;

// Sentinel for an unrestricted gap; large enough to leave antialiased coverage unchanged.
const OPEN_GAP: f32 = 1e6;

struct WallSurface {
    // Signed distance to the nearest boundary in cell units, positive inside a surface.
    distance: f32,
    // Nearest wall's style, used to shade fragments outside a surface.
    style: u32,
    // Surface height at the nearest edge, from 0 at ground level to 1 at a wall top.
    height: f32,
    // Height of the step at the nearest edge, from 0 to 1.
    edge_step: f32,
    // Gap width beyond the nearest edge, in world pixels; OPEN_GAP if unrestricted.
    gap: f32,
    // Outward edge normal within a ramp cell; zero for flat cells.
    edge_normal: vec2<f32>,
}

fn closer(a: WallSurface, b: WallSurface) -> WallSurface {
    if b.distance < a.distance { return b; }
    return a;
}

// Classifies the height of `cell` at `side`; returns EDGE_NONE for a ramp's sloping side.
fn edge_height(cell: u32, side: vec2<i32>) -> u32 {
    if cell_style(cell) == 0u { return EDGE_GROUND; }
    let ramp = cell_ramp(cell);
    if ramp == 0u { return EDGE_WALL_TOP; }
    let up = ramp_up(ramp);
    if all(side == up) { return EDGE_WALL_TOP; }
    if all(side == -up) { return EDGE_GROUND; }
    return EDGE_NONE;
}

// Checks whether cells across `offset` meet at the same constant height with compatible styles.
fn connects(cell: u32, neighbour_cell: u32, offset: vec2<i32>) -> bool {
    let height = edge_height(cell, offset);
    if height == EDGE_NONE || height != edge_height(neighbour_cell, -offset) { return false; }
    let style = cell_style(cell);
    let neighbour = cell_style(neighbour_cell);
    return style == neighbour || style == 0u || neighbour == 0u;
}

// Builds the surface field within a ramp cell. Beside a flat wall of the same style, the gap
// and edge details narrow toward the crest. Adjacent ramps of the same style and direction
// merge into one continuous slope.
fn ramp_surface(coords: vec2<i32>, cell: u32, p: vec2<f32>) -> WallSurface {
    let region = cell_style(cell);
    let ramp = cell_ramp(cell);
    let up_cell = ramp_up(ramp);
    let up = vec2<f32>(up_cell);
    let across_axis = vec2<f32>(-up.y, up.x);
    let along = dot(p, up);
    let across = dot(p, across_axis);
    let rise = along + 0.5;

    let side_sign = select(-1.0, 1.0, across >= 0.0);
    let side_normal = across_axis * side_sign;
    let side_cell = wall_cell_at(coords + vec2<i32>(side_normal));
    let side_style = cell_style(side_cell);
    let side_ramp = cell_ramp(side_cell);
    let beside_flat_wall = side_style == region && side_ramp == 0u;
    let beside_same_ramp = side_style == region && side_ramp == ramp;

    let inset = select(BASE_INSET, 0.0, beside_same_ramp) * (1.0 - rise);
    let side_distance = 0.5 - inset / CELL_SIZE - abs(across);
    var side_step = select(1.0, 1.0 - rise, beside_flat_wall);
    var side_gap = select(OPEN_GAP, BASE_INSET * (1.0 - rise), beside_flat_wall);
    if beside_same_ramp {
        side_step = 0.0;
        side_gap = 0.0;
    }
    var best = WallSurface(side_distance, region, rise, side_step, side_gap, side_normal);

    // Within the gap, use the neighboring wall if it is closer than the ramp.
    let to_cell_edge = abs(across) - 0.5;
    if side_distance < 0.0 && side_style != 0u && to_cell_edge > side_distance {
        let side_height = select(1.0, ramp_height(side_ramp, p - side_normal), side_ramp != 0u);
        best = WallSurface(to_cell_edge, side_style, side_height, side_step, side_gap, side_normal);
    }

    if !connects(cell, wall_cell_at(coords + up_cell), up_cell) {
        best = closer(best, WallSurface(0.5 - along, region, 1.0, 1.0, OPEN_GAP, up));
    }
    if !connects(cell, wall_cell_at(coords - up_cell), -up_cell) {
        best = closer(best, WallSurface(along + 0.5, region, 1.0, 1.0, OPEN_GAP, -up));
    }
    return best;
}

// Samples the surface boundary and edge properties for the packed `cell` at `coords`.
// `p` is relative to the cell center, in [-0.5, 0.5] on each axis.
fn wall_surface(coords: vec2<i32>, cell: u32, p: vec2<f32>) -> WallSurface {
    if cell_ramp(cell) != 0u { return ramp_surface(coords, cell, p); }
    let region = cell_style(cell);

    let toward_positive = vec2<f32>(0.5) - p;
    let toward_negative = vec2<f32>(0.5) + p;

    var cardinals = array<vec2<i32>, 4>(vec2<i32>(1, 0), vec2<i32>(-1, 0), vec2<i32>(0, 1), vec2<i32>(0, -1));
    var diagonals = array<vec2<i32>, 4>(vec2<i32>(1, 1), vec2<i32>(-1, 1), vec2<i32>(1, -1), vec2<i32>(-1, -1));

    var best = WallSurface(DISTANCE_CAP, 0u, 1.0, 1.0, OPEN_GAP, vec2<f32>(0.0));

    for (var i = 0; i < 4; i++) {
        let offset = cardinals[i];
        // Identical flat cells connect directly; ramp neighbors also need an edge-height check.
        let neighbour_cell = wall_cell_at(coords + offset);
        if neighbour_cell == cell { continue; }
        let neighbour_ramp = cell_ramp(neighbour_cell);
        if neighbour_ramp != 0u && connects(cell, neighbour_cell, offset) { continue; }
        let neighbour = cell_style(neighbour_cell);
        let edges = select(toward_negative, toward_positive, offset > vec2<i32>(0));
        var edge = select(edges.y, edges.x, offset.x != 0);

        var height = 1.0;
        var edge_step = 1.0;
        var gap = OPEN_GAP;
        if neighbour_ramp != 0u && edge_height(neighbour_cell, -offset) == EDGE_NONE {
            let rise = ramp_height(neighbour_ramp, p - vec2<f32>(offset));
            if region == 0u {
                // Measure to the inset ramp edge rather than the cell boundary.
                edge += BASE_INSET / CELL_SIZE * (1.0 - rise);
                height = rise;
            } else if neighbour == region {
                edge_step = 1.0 - rise;
                gap = BASE_INSET * edge_step;
            }
        }
        best = closer(best, WallSurface(edge, neighbour, height, edge_step, gap, vec2<f32>(0.0)));
    }

    for (var i = 0; i < 4; i++) {
        let offset = diagonals[i];
        let neighbour_cell = wall_cell_at(coords + offset);
        let neighbour = cell_style(neighbour_cell);
        if neighbour == region { continue; }
        let edge = select(toward_negative, toward_positive, offset > vec2<i32>(0));
        let neighbour_ramp = cell_ramp(neighbour_cell);
        var height = 1.0;
        if neighbour_ramp != 0u {
            height = ramp_height(neighbour_ramp, p - vec2<f32>(offset));
        }
        best = closer(best, WallSurface(max(length(edge), 1e-5), neighbour, height, 1.0, OPEN_GAP, vec2<f32>(0.0)));
    }

    best.distance *= select(-1.0, 1.0, region != 0u);
    return best;
}

fn surface_at(world: vec2<f32>) -> WallSurface {
    let f = world / CELL_SIZE;
    let base = floor(f);
    let coords = vec2<i32>(base);
    return wall_surface(coords, wall_cell_at(coords), f - base - vec2<f32>(0.5));
}

@fragment
fn fragment(mesh: VertexOutput) -> @location(0) vec4<f32> {
    let f = mesh.uv * vec2<f32>(grid_bounds());
    let base = floor(f);
    let coords = vec2<i32>(base);
    let world = f * CELL_SIZE;

    // World-coordinate span of one screen pixel; evaluated before divergent control flow.
    let texel = max(max(fwidth(world.x), fwidth(world.y)), 0.0001);

    let cell = wall_cell_at(coords);
    let region = cell_style(cell);
    let ramp = cell_ramp(cell);
    let p = f - base - vec2<f32>(0.5);
    let surface = wall_surface(coords, cell, p);

    // Inside a wall the pixel belongs to its own style; on open ground it belongs to
    // whichever wall is nearest, which may be one of several bordering that cell.
    let style_index = select(surface.style, region, region != 0u);
    if style_index == 0u { return vec4<f32>(0.0); }
    let style = wall_style(style_index);

    let raw_distance = surface.distance * CELL_SIZE;

    // Diagnostics evaluate erosion and lighting even outside the visible edge layers.
    let debugging = settings.debug_mode != DEBUG_OFF;
    let edge_reach = edge_layers_reach(texel, style);

    // Skip erosion where its maximum displacement cannot reach a visible edge layer.
    let erosion = style.geometry.erosion_amount;
    var d = raw_distance;
    if erosion > 0.0 && (debugging || abs(raw_distance) < edge_reach + erosion) {
        d = eroded_distance(raw_distance, world, surface.edge_step, style);
    }

    var coverage = smoothstep(-texel, texel, d);

    // Limit transparency to the gap's width within the antialiasing span.
    coverage = 1.0 - (1.0 - coverage) * saturate(surface.gap / (texel * 2.0));

    var tone = 1.0;
    if ramp != 0u {
        let up = ramp_up(ramp);
        let rise = ramp_height(ramp, p);
        let sun_facing = -dot(vec2<f32>(up), MAP_SUN_GROUND_DIRECTION);
        tone = ramp_tone(rise, sun_facing);

        // Use a sharp brightness change where the ramp meets a surface at its crest.
        if connects(cell, wall_cell_at(coords + up), up) {
            tone *= 1.0 + CREST_BREAK * sun_facing;
        }

        // Fade coverage at the foot where the ramp meets open ground.
        if wall_style_index_at(coords - up) == 0u {
            coverage *= smoothstep(0.0, texel * 2.0, rise * CELL_SIZE);
        }
    }

    // Sample toward the sun for a contact shadow. Lower surfaces cast shorter shadows,
    // reducing the shadow to zero at a ramp's foot.
    var shadow = 0.0;
    let shadow_length = style.surface.shadow_length;
    if coverage < 1.0 && raw_distance > -shadow_length * SHADOW_REACH {
        let shadow_offset = MAP_SUN_GROUND_DIRECTION * shadow_length * SHADOW_OFFSET;
        let caster = surface_at(world + shadow_offset);
        let shadow_distance = min(caster.distance * CELL_SIZE, 0.0) - shadow_length * (1.0 - caster.height);
        shadow = pow(saturate(1.0 + shadow_distance / shadow_length), 1.5) * SHADOW_STRENGTH * (1.0 - coverage);
    }

    let alpha = coverage + shadow * (1.0 - coverage);
    if alpha < 0.002 { return vec4<f32>(0.0); }

    // Diagnostics draw the wall only, so the shape stays readable against the map.
    if debugging && coverage < 0.5 { return vec4<f32>(0.0); }

    // Pure shadow contributes only black and alpha.
    if coverage <= 0.0 { return vec4<f32>(0.0, 0.0, 0.0, alpha); }

    // Ramp edges use explicit normals. Flat edges estimate lighting from the uneroded field;
    // a probe at least one screen pixel wide smooths corners and avoids aliasing when zoomed out.
    var lit = 0.0;
    if ramp != 0u {
        lit = dot(surface.edge_normal, MAP_SUN_GROUND_DIRECTION);
    } else if debugging || d < edge_reach {
        let probe = max(LIGHT_PROBE, texel);
        lit = -(surface_at(world + MAP_SUN_GROUND_DIRECTION * probe).distance * CELL_SIZE - raw_distance) / probe;
    }
    lit *= surface.edge_step;

    let plate = plate_noise(world, style);
    let colour = wall_shading(d, lit, plate, texel, tone, surface.edge_step, style);

    if debugging {
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
