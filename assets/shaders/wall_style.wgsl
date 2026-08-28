#define_import_path dwd::wall_style
#import dwd::gradient_noise::dwd_gradient_fbm_2d

// Binding-free wall style layout and shading functions. Callers own their material bindings, so the
// same code can serve both the map canvas and UI swatches.

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
}

// Field order and types must match `WallStyle` in map_objects/src/wall_style.rs.
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
// Body variation is exposed separately for the canvas noise diagnostic.
fn plate_noise(world: vec2<f32>, style: WallStyle) -> f32 {
    return dwd_gradient_fbm_2d(world / max(style.surface.plate_noise_scale, 0.0001), 4);
}

// Perturbs signed distance before coverage and layer thresholds. Facing is sampled from the raw
// field so high-frequency erosion does not affect lighting.
fn eroded_distance(raw_distance: f32, world: vec2<f32>, style: WallStyle) -> f32 {
    return raw_distance + (dwd_gradient_fbm_2d(world / GRAIN_SCALE, 3) - 0.5) * style.geometry.erosion_amount;
}

// Composites body, bevel, hairline and contour. `d` is signed world-pixel distance, `lit` is the
// sun-facing estimate, and `texel` is world pixels per screen pixel.
fn wall_shading(d: f32, lit: f32, plate: f32, texel: f32, style: WallStyle) -> vec3<f32> {
    // Fade edge-facing before the capped distance field becomes flat. Smooth endpoints prevent a
    // visible interior contour at the end of the ramp.
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
