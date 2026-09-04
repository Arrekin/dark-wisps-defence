#import bevy_ui::ui_vertex_output::{UiVertexOutput}
#import bevy_render::globals::{Globals}

// Parameterized UI material using pixel-space signed-distance fields with derivative-based
// antialiasing. It composites a field vignette, contour, hairline, rim, and optional corner mark.
// Hover and selection adjust layer intensity and tint without changing the panel interior.
//
// Bind group 0: view uniform (binding 0) + globals (binding 1).
// Bind group 1: material uniform (binding 0).

@group(0) @binding(1)
var<uniform> globals: Globals;

// Silhouette dimensions in pixels and the resting intensity of the three edge layers.
struct VoidPanelGeometry {
    border_width: f32,
    corner_cut: f32,
    edge_brightness: f32,
    rim_intensity: f32,
    hairline_strength: f32,
    // How far the contour's brightness swings around 1.0 between the facet facing the light and
    // the one facing away. Zero draws every edge the same colour.
    //
    // Declared as 12 bytes so the struct measures a multiple of 16 and the members after it in
    // VoidPanelMaterial keep their offsets.
    @size(12) contour_light_range: f32,
};

// How the panel answers a fully raised style state. The scales multiply the field and the
// contour intensity — below 1 recedes, above 1 asserts — and `tint` is how far the contour
// moves toward style_color. `corner_mark` is the width in pixels of a wedge of style_color
// in the bottom-right corner, 0 for none. (1, 1, 0, 0) is inert.
struct VoidPanelStyleResponse {
    field_scale: f32,
    contour_scale: f32,
    tint: f32,
    corner_mark: f32,
};

// One state's fade, evaluated by `eased()`. The rate travels with the fade so the curve is
// defined once, on the Rust side.
struct Fade {
    start_value: f32,
    end_value: f32,
    start_time: f32,
    rate: f32,
};

// Shape and travel of two surges that move continuously around the panel border.
struct VoidPanelBorderSurge {
    // Laps of the perimeter per second.
    rate: f32,
    // Length of one surge along the border, in pixels.
    span: f32,
    // Pixels added to contour width at the surge center. Capped before the contour reaches
    // HAIRLINE_INSET, preserving the dark channel below the hairline.
    width: f32,
    // How hard the surge drives the contour's brightness.
    intensity: f32,
};

// Mirrors VoidPanelMaterial in widgets/src/void_panel.rs field for field.
struct VoidPanelMaterial {
    background_center: vec4<f32>,
    background_edge: vec4<f32>,
    border_color: vec4<f32>,
    accent_color: vec4<f32>,
    // Tint the style state carries (linear rgb; alpha unused).
    style_color: vec4<f32>,
    geometry: VoidPanelGeometry,
    style_response: VoidPanelStyleResponse,
    selected_fade: Fade,
    hover_fade: Fade,
    style_fade: Fade,
    border_surge_fade: Fade,
    border_surge: VoidPanelBorderSurge,
};

@group(1) @binding(0)
var<uniform> material: VoidPanelMaterial;

const SQRT_2_INV: f32 = 0.70710678;

// Light direction in y-down UI space.
const LIGHT_DIR: vec2<f32> = vec2<f32>(-0.4472, -0.8944);

// Distance from the border at which the inner hairline sits.
const HAIRLINE_INSET: f32 = 4.0;

// How far hover alone carries the tint toward the accent; selection carries it fully.
const HOVER_TINT: f32 = 0.5;

// Per-layer interaction weights for hover and selection.
const CONTOUR_HOVER: f32 = 0.5;
const CONTOUR_SELECTED: f32 = 1.2;
const RIM_HOVER: f32 = 0.29;
const RIM_SELECTED: f32 = 0.39;

// Corner-mark brightness.
const MARK_INTENSITY: f32 = 0.9;

// Exponential scale length of the inner rim, in pixels.
const RIM_SCALE: f32 = 8.0;

// Current value of a fade.
//
// Interpolate fades from `globals.time` for framerate-independent animation.
fn eased(fade: Fade, now: f32) -> f32 {
    let elapsed = max(0.0, now - fade.start_time);
    return mix(fade.start_value, fade.end_value, 1.0 - exp(-fade.rate * elapsed));
}


// Position around the silhouette, 0..1, clockwise from the top-left corner.
//
// Runs are weighted by their own length, so a surge crosses a short edge and a long one
// at the same speed. Measuring on the box and ignoring the corner cuts costs a fraction of
// a pixel of drift at each chamfer, which is invisible at any panel size we draw.
fn perimeter_position(p: vec2<f32>, half: vec2<f32>) -> f32 {
    let width = half.x * 2.0;
    let height = half.y * 2.0;
    let perimeter = 2.0 * (width + height);

    let along_top = p.x + half.x;
    let along_right = width + p.y + half.y;
    let along_bottom = width + height + half.x - p.x;
    let along_left = 2.0 * width + height + half.y - p.y;

    let horizontal = (half.y - abs(p.y)) <= (half.x - abs(p.x));
    let horizontal_run = select(along_bottom, along_top, p.y < 0.0);
    let vertical_run = select(along_left, along_right, p.x > 0.0);
    return select(vertical_run, horizontal_run, horizontal) / perimeter;
}

// Shortest distance between two positions on a ring, in laps.
fn ring_gap(a: f32, b: f32) -> f32 {
    return abs(fract(a - b + 0.5) - 0.5);
}

// Signed distance to a box chamfered on the top-left / bottom-right diagonal only.
// `abs(p.x + p.y)` is the fold that selects that corner pair; `abs(p.x - p.y)` would
// select the other diagonal.
//
// Exact inside the shape, which is all that is sampled, but an underestimate outside the
// chamfered corners — adequate for a mask and a 1px antialiasing band.
fn sd_panel(p: vec2<f32>, half: vec2<f32>, cut: f32) -> f32 {
    let q = abs(p);
    let box_d = max(q.x - half.x, q.y - half.y);
    let chamfer_d = (abs(p.x + p.y) - (half.x + half.y - cut)) * SQRT_2_INV;
    return max(box_d, chamfer_d);
}

@fragment
fn fragment(in: UiVertexOutput) -> @location(0) vec4<f32> {
    let uv = in.uv;
    let size = in.size;
    // Inset by 1px so the antialiasing band fits inside the node's quad rather than being
    // clipped in half at its boundary.
    let half_size = size * 0.5 - 1.0;
    let p = (uv - vec2<f32>(0.5)) * size;

    let border_width = material.geometry.border_width;
    let corner_cut = material.geometry.corner_cut;
    let edge_brightness = material.geometry.edge_brightness;
    let rim_intensity = material.geometry.rim_intensity;
    let hairline_strength = material.geometry.hairline_strength;
    let selected = eased(material.selected_fade, globals.time);
    let hover = eased(material.hover_fade, globals.time);
    let style_amount = eased(material.style_fade, globals.time);
    let style_field = material.style_response.field_scale;
    let style_contour = material.style_response.contour_scale;
    let style_tint = material.style_response.tint;
    let style_mark = material.style_response.corner_mark;
    let surge_amount = eased(material.border_surge_fade, globals.time);

    // ---- Silhouette ----
    let d = sd_panel(p, half_size, corner_cut);
    let aa = fwidth(d) * 0.7;
    let coverage = 1.0 - smoothstep(-aa, aa, d);

    if (coverage < 0.001) {
        return vec4<f32>(0.0);
    }

    // Distance inward from the border. Every layer below is a function of this, so all of
    // them follow the chamfered silhouette.
    let inset = -d;

    // ---- Facet normals ----
    // The SDF gradient is piecewise constant, one value per flat face of the octagon.
    // That suits a 1px contour, which is where it is applied, rather than a gradient
    // spread across the panel.
    let n = normalize(vec2<f32>(dpdx(d), dpdy(d)) + vec2<f32>(1e-6));
    let n_dot_l = dot(n, LIGHT_DIR);
    // Directional contour lighting. With range 0.5, brightness is approximately 1.45 on the top
    // edge and 0.55 on the bottom edge before applying edge_brightness.
    let contour_light = 1.0 + n_dot_l * material.geometry.contour_light_range;
    // How far this facet faces up. Confines the hairline to the top edge and fades it
    // around the corners instead of ending it abruptly.
    let top_weight = clamp(-n.y, 0.0, 1.0);

    // ---- Interaction ----
    // Hue has two positions, structural border and accent, so the states compete rather
    // than accumulate. Escalation past full accent is carried by intensity below.
    let activation = max(selected, hover * HOVER_TINT);
    let accent_tint = mix(material.border_color.rgb, material.accent_color.rgb, activation);

    // ---- 1. Field ----
    // Vignette from the border inward: edges fall to near-black, the center lifts. Reads
    // as depth without simulating geometry, and follows the silhouette automatically.
    let vignette = smoothstep(0.0, 1.0, clamp(inset / (min(size.x, size.y) * 0.5), 0.0, 1.0));
    var field = mix(material.background_edge.rgb, material.background_center.rgb, vignette);

    field *= mix(1.0, style_field, style_amount);

    var color = field;

    // ---- 2. Contour ----
    // A band on the inside of the silhouette, not a fill mask. Dim at rest so a screen of
    // panels reads as plates separated by value rather than by outlines; brightness here
    // is reserved for hover and selection.
    //
    // Two surges travel half a lap apart and increase the contour's width and intensity.
    let perimeter = 2.0 * (size.x + size.y);
    let lap = material.border_surge.rate * globals.time;
    let here = perimeter_position(p, half_size);
    let nearest_surge = min(ring_gap(here, lap), ring_gap(here, lap + 0.5));
    let surge = (1.0 - smoothstep(0.0, material.border_surge.span / perimeter, nearest_surge)) * surge_amount;

    let contour = 1.0 - smoothstep(
        border_width + surge * material.border_surge.width - aa,
        border_width + surge * material.border_surge.width + aa,
        inset,
    );
    let contour_color = mix(accent_tint, material.style_color.rgb, style_amount * style_tint);
    let contour_intensity = edge_brightness
        * (1.0 + hover * CONTOUR_HOVER + selected * CONTOUR_SELECTED
            + surge * material.border_surge.intensity)
        * mix(1.0, style_contour, style_amount);
    color += contour * contour_color * contour_intensity * contour_light;

    // ---- 3. Hairline ----
    // A crisp line a few pixels below the top edge. Together with the rim it gives the
    // border apparent thickness.
    let hairline = (1.0 - smoothstep(0.5, 0.5 + aa, abs(inset - HAIRLINE_INSET))) * top_weight;
    color += hairline * material.border_color.rgb * hairline_strength;

    // ---- 4. Rim ----
    // Exponential falloff holds closer to the border than a linear ramp. Near-invisible at
    // rest, where it only keeps the contour from looking pasted onto the field; on hover
    // and selection it carries most of the response.
    let rim = exp(-inset / RIM_SCALE);
    let rim_strength = rim_intensity + hover * RIM_HOVER + selected * RIM_SELECTED;
    color += rim * accent_tint * rim_strength;

    // ---- 5. Corner mark ----
    // A band lying against the inside of the bottom-right chamfer. Unlike the layers above
    // it adds light rather than scaling light that is already there, which is why it stays
    // legible across a grid: the field and contour sit near black, so scaling them moves
    // almost nothing.
    //
    // `p.x + p.y` is the same diagonal `sd_panel` folds with `abs()` to cut both corners.
    // Without the fold it grows toward the bottom-right only, so the band lands in that one
    // corner and nowhere else.
    // A width of 0 must draw nothing at all. Without the `select` the antialiasing band
    // still straddles the chamfer, leaving a hairline on every panel whose style is raised
    // but that asked for no mark.
    let br_diagonal = ((p.x + p.y) - (half_size.x + half_size.y - corner_cut)) * SQRT_2_INV;
    let mark_band = smoothstep(-style_mark - aa, -style_mark + aa, br_diagonal);
    let mark = select(0.0, mark_band, style_mark > 0.0);
    color += mark * material.style_color.rgb * style_amount * MARK_INTENSITY;

    return vec4<f32>(color, coverage);
}
