// Wall style layout and shading shared by the map canvas and UI swatches.
// Callers supply material bindings.

import package::shaders::common::core::{CELL_HALF, CELL_SIZE};
import package::shaders::common::gradient_noise::gradient_fbm_2d;
import package::shaders::common::map_light::MAP_SUN_GROUND_DIRECTION;

// Distances and erosion amplitude are measured in world pixels.
struct WallStyleGeometry {
    bevel_width: f32,
    contour_width: f32,
    hairline_width: f32,
    erosion_amount: f32,
}

// Noise lattice size and shadow length are measured in world pixels.
struct WallStyleSurface {
    plate_noise_scale: f32,
    shadow_length: f32,
    // Matches the 16-byte layout of WallStyleSurface in Rust.
    _padding: vec2<f32>,
}

// Field order and types mirror `WallStyle` in map_objects/src/wall_style.rs.
struct WallStyle {
    body_low: vec4<f32>,
    body_high: vec4<f32>,
    bevel_color: vec4<f32>,
    hairline_color: vec4<f32>,
    contour_color: vec4<f32>,
    geometry: WallStyleGeometry,
    surface: WallStyleSurface,
}

// Base edge-erosion noise lattice size in world pixels.
const GRAIN_SCALE: f32 = 9.0;
// Dimensionless sun-facing response coefficients.
const BEVEL_LIGHT: f32 = 0.55;
const BODY_LIGHT: f32 = 0.10;
// Minimum contour and hairline width in screen pixels.
const MIN_EDGE_TEXELS: f32 = 1.2;
// Facing-probe distance in world pixels; also controls mitre smoothing width.
const LIGHT_PROBE: f32 = 2.0;

// Inset on each side of the ramp at its low edge, in world pixels.
const BASE_INSET: f32 = 3.0;
// Base body brightness at the ramp's foot, relative to its crest.
const FOOT_TONE: f32 = 0.12;
// Strength of directional lighting at the ramp's foot, fading to zero at its crest.
const TILT_LIGHT: f32 = 0.15;

// Brightness change at a connected crest, scaled by the slope's orientation toward the sun.
const CREST_BREAK: f32 = 0.3;

// Converts a nonzero ramp code to its uphill grid direction; positive y points up.
// Mirrors `Wall::ramp_gpu_code` in map_objects/src/lib.rs.
fn ramp_up(ramp: u32) -> vec2<i32> {
    switch ramp {
        case 1u: { return vec2<i32>(1, 0); }
        case 2u: { return vec2<i32>(-1, 0); }
        case 3u: { return vec2<i32>(0, 1); }
        default: { return vec2<i32>(0, -1); }
    }
}

// Returns height in [0, 1]. `p` is in cell units relative to the ramp cell's center.
fn ramp_height(ramp: u32, p: vec2<f32>) -> f32 {
    return saturate(dot(p, vec2<f32>(ramp_up(ramp))) + 0.5);
}

// Body brightness multiplier at height `rise` in [0, 1].
// `sun_facing` ranges from -1 facing away from the sun to 1 facing toward it.
fn ramp_tone(rise: f32, sun_facing: f32) -> f32 {
    return mix(FOOT_TONE, 1.0, rise) * (1.0 + TILT_LIGHT * sun_facing * (1.0 - rise));
}

// Signed distance in world pixels to one isolated cell's box, positive inside.
fn single_cell_distance(world: vec2<f32>) -> f32 {
    let from_centre = abs(world - vec2<f32>(CELL_HALF));
    return CELL_HALF - max(from_centre.x, from_centre.y);
}

// Body variation is exposed separately for the canvas noise diagnostic.
fn plate_noise(world: vec2<f32>, style: WallStyle) -> f32 {
    return gradient_fbm_2d(world / style.surface.plate_noise_scale, 4);
}

// Perturbs signed distance with noise scaled by the style's erosion amount and edge-step height.
fn eroded_distance(raw_distance: f32, world: vec2<f32>, edge_step: f32, style: WallStyle) -> f32 {
    return raw_distance + (gradient_fbm_2d(world / GRAIN_SCALE, 3) - 0.5) * style.geometry.erosion_amount * edge_step;
}

// Maximum extent of edge shading into the surface, in world pixels.
fn edge_layers_reach(texel: f32, style: WallStyle) -> f32 {
    let least = texel * MIN_EDGE_TEXELS;
    let lines = max(style.geometry.contour_width, least) + max(style.geometry.hairline_width, least) * 2.0;
    return max(style.geometry.bevel_width * 2.0, lines);
}

// Returns (width in world pixels, opacity multiplier) for a line scaled by `edge_step`.
// Below the minimum screen width, reduce opacity instead of width to keep the line continuous.
fn scaled_line(width: f32, edge_step: f32, texel: f32) -> vec2<f32> {
    let least = texel * MIN_EDGE_TEXELS;
    let scaled = max(width, least) * edge_step;
    if scaled >= least { return vec2<f32>(scaled, 1.0); }
    return vec2<f32>(least, scaled / least);
}

// Composites body, bevel, hairline and contour. `d` is signed distance in world pixels, positive
// inside the surface. `lit` is edge lighting, and `texel` is world pixels per screen pixel.
// `plate` blends the body colors; `tone` scales body brightness.
// `edge_step` is the height of the step at the nearest edge, from 0 to 1; edge widths scale with it.
fn wall_shading(d: f32, lit: f32, plate: f32, texel: f32, tone: f32, edge_step: f32, style: WallStyle) -> vec3<f32> {
    let bevel_width = max(style.geometry.bevel_width * edge_step, 0.0001);
    // Fade directional body lighting into the interior without a visible transition line.
    let facet = lit * (1.0 - smoothstep(0.0, bevel_width * 2.0, d));
    var colour = mix(style.body_low.rgb, style.body_high.rgb, saturate(plate)) * (1.0 + BODY_LIGHT * facet) * tone;

    // Bevel: the chamfer between the contour and the flat top face.
    let bevel_progress = smoothstep(0.0, bevel_width, d);
    let bevel = style.bevel_color.rgb * (1.0 + BEVEL_LIGHT * lit);
    colour = mix(bevel, colour, bevel_progress);

    let contour_line = scaled_line(style.geometry.contour_width, edge_step, texel);
    let hairline_line = scaled_line(style.geometry.hairline_width, edge_step, texel);
    let contour_width = contour_line.x;
    let hairline_width = hairline_line.x;

    // Bright hairline just inside the contour, brightest on edges facing the light.
    let hairline = saturate(1.0 - abs(d - (contour_width + hairline_width * 0.5)) / hairline_width)
        * (0.45 + 0.55 * saturate(lit)) * hairline_line.y;
    colour = colour + style.hairline_color.rgb * hairline;

    let contour = saturate(1.0 - abs(d - contour_width * 0.5) / contour_width) * contour_line.y;
    return mix(colour, style.contour_color.rgb, contour);
}

// Shades an isolated flat wall cell, returning color and coverage as alpha, without a contact shadow.
// `world` is in world pixels from the cell's bottom-left corner; `texel` is world pixels per screen pixel.
fn single_cell_wall(world: vec2<f32>, texel: f32, style: WallStyle) -> vec4<f32> {
    let raw_distance = single_cell_distance(world);
    let d = eroded_distance(raw_distance, world, 1.0, style);

    let probe = max(LIGHT_PROBE, texel);
    let lit = -(single_cell_distance(world + MAP_SUN_GROUND_DIRECTION * probe) - raw_distance) / probe;

    let colour = wall_shading(d, lit, plate_noise(world, style), texel, 1.0, 1.0, style);
    return vec4<f32>(colour, smoothstep(-texel, texel, d));
}

// Shades an isolated ramp, returning color and coverage as alpha. The high edge has a contour;
// the low edge fades into the ground. `world` is in world pixels from the cell's bottom-left corner;
// `texel` is world pixels per screen pixel, and `ramp` must be a nonzero ramp code.
fn single_cell_ramp(world: vec2<f32>, texel: f32, ramp: u32, style: WallStyle) -> vec4<f32> {
    let p = world / CELL_SIZE - vec2<f32>(0.5);
    let up = vec2<f32>(ramp_up(ramp));
    let across_axis = vec2<f32>(-up.y, up.x);
    let across = dot(p, across_axis);
    let rise = ramp_height(ramp, p);

    let side_distance = (0.5 - abs(across)) * CELL_SIZE - BASE_INSET * (1.0 - rise);
    let high_distance = (0.5 - dot(p, up)) * CELL_SIZE;
    let side_normal = across_axis * select(-1.0, 1.0, across >= 0.0);
    let edge_normal = select(up, side_normal, side_distance < high_distance);

    let d = eroded_distance(min(side_distance, high_distance), world, 1.0, style);
    let lit = dot(edge_normal, MAP_SUN_GROUND_DIRECTION);
    let tone = ramp_tone(rise, -dot(up, MAP_SUN_GROUND_DIRECTION));
    let colour = wall_shading(d, lit, plate_noise(world, style), texel, tone, 1.0, style);

    // Fade coverage at the foot without drawing a contour.
    let coverage = smoothstep(-texel, texel, d) * smoothstep(0.0, texel * 2.0, rise * CELL_SIZE);
    return vec4<f32>(colour, coverage);
}
