#define_import_path dwd::wall_style

// Wall style
//
// The parameter set a wall is drawn with, and the layers that turn a signed distance into a
// colour. Everything here is a pure function of values the caller already holds — nothing in
// this file reads a binding, and that is deliberate: `Material2d` binds at `@group(2)` and
// `UiMaterial` at `@group(1)`, so a module shared between the map canvas and a UI swatch can
// declare no bindings at all. Whatever touches a binding stays in the shader that owns it.

struct WallStyleGeometry {
    bevel_width: f32,
    contour_width: f32,
    hairline_width: f32,
    erosion_amount: f32,
}

struct WallStyleSurface {
    plate_noise_scale: f32,
    shadow_length: f32,
    light_direction: vec2<f32>,
}

// Field order and types mirror `WallStyle` in map_objects/src/wall_style.rs exactly.
// A mismatch compiles clean and renders garbage, once per style.
struct WallStyle {
    body_low: vec4<f32>,
    body_high: vec4<f32>,
    bevel_color: vec4<f32>,
    hairline_color: vec4<f32>,
    contour_color: vec4<f32>,
    geometry: WallStyleGeometry,
    surface: WallStyleSurface,
}

// World pixels per octave of the edge-erosion noise.
const GRAIN_SCALE: f32 = 9.0;
// How hard the facing term drives the bevel and the flat top face.
const BEVEL_LIGHT: f32 = 0.55;
const BODY_LIGHT: f32 = 0.10;
// Contour and hairline never thin below this many screen pixels, so walls stay legible at
// the far end of the zoom range where one screen pixel covers four world pixels.
const MIN_EDGE_TEXELS: f32 = 1.2;
// World pixels the facing probe steps along the light. Also the width over which a mitre
// blends from one face to the other.
const LIGHT_PROBE: f32 = 2.0;
// Scales the signed noise to roughly a unit spread before centring, so `plate` lands within
// [0, 1] and a style can treat it as a plain blend factor.
const NOISE_GAIN: f32 = 1.2;

// Gradient noise: a direction per lattice point, and zero at the lattice points themselves, so
// the contours do not follow the grid. The body texture is spread over cells the same order of
// size as the noise lattice, where anything grid-aligned reads as blocks drawn on the wall.
fn gradient_at(cell: vec2<f32>) -> vec2<f32> {
    let seed = vec2<f32>(dot(cell, vec2<f32>(127.1, 311.7)), dot(cell, vec2<f32>(269.5, 183.3)));
    return fract(sin(seed) * 43758.5453) * 2.0 - vec2<f32>(1.0);
}

fn gradient_noise(p: vec2<f32>) -> f32 {
    let base = floor(p);
    let f = p - base;
    // Quintic fade: first and second derivatives are both zero at the lattice lines, so the
    // lattice leaves no visible band.
    let u = f * f * f * (f * (f * 6.0 - 15.0) + 10.0);
    let a = dot(gradient_at(base), f);
    let b = dot(gradient_at(base + vec2<f32>(1.0, 0.0)), f - vec2<f32>(1.0, 0.0));
    let c = dot(gradient_at(base + vec2<f32>(0.0, 1.0)), f - vec2<f32>(0.0, 1.0));
    let d = dot(gradient_at(base + vec2<f32>(1.0, 1.0)), f - vec2<f32>(1.0, 1.0));
    return mix(mix(a, b, u.x), mix(c, d, u.x), u.y) * NOISE_GAIN + 0.5;
}

// Sampled in world space, never in cell space: cell-space noise puts a seam on every cell
// border, and cell borders are meant to be invisible.
//
// Each octave is rotated as well as doubled. Octaves sharing a lattice orientation reinforce
// each other's axes and the sum acquires a grid.
fn fbm(p: vec2<f32>, octaves: i32) -> f32 {
    let rotation = mat2x2<f32>(0.8, -0.6, 0.6, 0.8);
    var total = 0.0;
    var amplitude = 0.5;
    var point = p;
    for (var i = 0; i < octaves; i++) {
        total = total + amplitude * gradient_noise(point);
        point = rotation * point * 2.0;
        amplitude = amplitude * 0.5;
    }
    return total;
}

// The large-scale variation across the body. Separate from `wall_shading` because the caller
// also wants it on its own for the noise diagnostic.
fn plate_noise(world: vec2<f32>, style: WallStyle) -> f32 {
    return fbm(world / max(style.surface.plate_noise_scale, 0.0001), 4);
}

// Chips the silhouette. Applied to the distance before anything thresholds it, so coverage and
// every layer erode together. The caller adds it after taking its facing probe, so this
// high-frequency noise does not shake the light.
fn eroded_distance(raw_distance: f32, world: vec2<f32>, style: WallStyle) -> f32 {
    return raw_distance + (fbm(world / GRAIN_SCALE, 3) - 0.5) * style.geometry.erosion_amount;
}

// Body, bevel, hairline and contour, in that order.
//
// `d` is the signed distance in world pixels, positive inside the wall. `lit` is how much the
// field falls away along the light. `plate` is [`plate_noise`]. `texel` is world pixels per
// screen pixel, which sets the floor on how thin an edge is allowed to get.
fn wall_shading(d: f32, lit: f32, plate: f32, texel: f32, style: WallStyle) -> vec3<f32> {
    // The facing term is faded out with distance because it only means anything near an edge —
    // deep inside a wall the field is flat and the probe returns nothing to shade with. The
    // iso-surface of `d` at a fixed distance is a square inside every cell, so a kink in this
    // ramp is drawn on the wall as a square; smoothstep is flat at both ends and has none.
    let facet = lit * (1.0 - smoothstep(0.0, max(style.geometry.bevel_width * 2.0, 0.0001), d));
    var colour = mix(style.body_low.rgb, style.body_high.rgb, saturate(plate)) * (1.0 + BODY_LIGHT * facet);

    // Bevel: the chamfer between the contour and the flat top face.
    let bevel_progress = smoothstep(0.0, max(style.geometry.bevel_width, 0.0001), d);
    let bevel = style.bevel_color.rgb * (1.0 + BEVEL_LIGHT * lit);
    colour = mix(bevel, colour, bevel_progress);

    let contour_width = max(style.geometry.contour_width, texel * MIN_EDGE_TEXELS);
    let hairline_width = max(style.geometry.hairline_width, texel * MIN_EDGE_TEXELS);

    // Bright hairline just inside the contour, brightest on edges facing the light.
    let hairline = saturate(1.0 - abs(d - (contour_width + hairline_width * 0.5)) / hairline_width)
        * (0.45 + 0.55 * saturate(lit));
    colour = colour + style.hairline_color.rgb * hairline;

    let contour = saturate(1.0 - abs(d - contour_width * 0.5) / contour_width);
    return mix(colour, style.contour_color.rgb, contour);
}
