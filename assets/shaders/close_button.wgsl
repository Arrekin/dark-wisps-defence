#import bevy_ui::ui_vertex_output::{UiVertexOutput}
#import bevy_render::globals::{Globals}

// Close mark with two asymmetrical crossed strokes. The surround appears only on hover.
// The heavier stroke follows the panel chamfer diagonal; the crossing gap keeps both strokes
// visually distinct.

@group(0) @binding(1)
var<uniform> globals: Globals;

// Must stay identical to `Fade` in the other UI shaders and on the Rust side.
struct Fade {
    start_value: f32,
    end_value: f32,
    start_time: f32,
    rate: f32,
};

struct CloseButtonGeometry {
    // Half-length of a stroke, in pixels: the mark spans twice this.
    mark_size: f32,
    // Stroke along the chamfer diagonal, and the one crossing it.
    heavy_width: f32,
    light_width: f32,
    // Half-width of the gap at the crossing, in pixels.
    gap: f32,
};

struct CloseButtonHover {
    // Radians the mark turns through by the time hover is full.
    rotation: f32,
    // Chamfer on the surround that appears under the pointer.
    corner_cut: f32,
    surround_edge: f32,
    surround_fill: f32,
};

struct CloseButtonMaterial {
    mark_color: vec4<f32>,
    hover_color: vec4<f32>,
    surround_color: vec4<f32>,
    geometry: CloseButtonGeometry,
    hover: CloseButtonHover,
    hover_fade: Fade,
};

@group(1) @binding(0)
var<uniform> material: CloseButtonMaterial;

const SQRT_2_INV: f32 = 0.70710678;
// Direction of the chamfer the panels are cut on, and the one crossing it.
const HEAVY_DIR: vec2<f32> = vec2<f32>(SQRT_2_INV, SQRT_2_INV);
const LIGHT_DIR: vec2<f32> = vec2<f32>(SQRT_2_INV, -SQRT_2_INV);

fn eased(fade: Fade, now: f32) -> f32 {
    let elapsed = max(0.0, now - fade.start_time);
    return mix(fade.start_value, fade.end_value, 1.0 - exp(-fade.rate * elapsed));
}

// Distance to a stroke lying along `dir`, present between `gap` and `reach` from the centre
// on both sides. The two arms are one expression because the projection is clamped by its
// magnitude and given its sign back.
fn stroke(p: vec2<f32>, dir: vec2<f32>, gap: f32, reach: f32) -> f32 {
    let projection = dot(p, dir);
    let along = clamp(abs(projection), gap, reach) * sign(projection);
    return length(p - dir * along);
}

// Signed distance to a box chamfered on the top-left / bottom-right diagonal, the silhouette
// every panel in the interface wears.
fn sd_surround(p: vec2<f32>, half: vec2<f32>, cut: f32) -> f32 {
    let q = abs(p);
    let box_d = max(q.x - half.x, q.y - half.y);
    let chamfer_d = (abs(p.x + p.y) - (half.x + half.y - cut)) * SQRT_2_INV;
    return max(box_d, chamfer_d);
}

@fragment
fn fragment(in: UiVertexOutput) -> @location(0) vec4<f32> {
    let size = in.size;
    let p = (in.uv - vec2<f32>(0.5)) * size;
    let hover = eased(material.hover_fade, globals.time);

    var color = vec3<f32>(0.0);
    var alpha = 0.0;

    // ---- Surround ----
    // Rendered behind the mark only while hovered.
    let half_size = size * 0.5 - 1.0;
    let d = sd_surround(p, half_size, material.hover.corner_cut);
    let surround_aa = fwidth(d) * 0.7;
    let inside = 1.0 - smoothstep(-surround_aa, surround_aa, d);
    let edge = 1.0 - smoothstep(1.0 - surround_aa, 1.0 + surround_aa, -d);
    let surround = inside * material.hover.surround_fill + edge * material.hover.surround_edge;
    color += material.surround_color.rgb * surround * hover;
    alpha = max(alpha, min(surround, 1.0) * hover * material.surround_color.a);

    // ---- Mark ----
    // Turning a few degrees under the pointer is the whole acknowledgement; nothing else
    // about the mark changes shape.
    let angle = hover * material.hover.rotation;
    let turn = mat2x2<f32>(cos(angle), -sin(angle), sin(angle), cos(angle));
    let q = turn * p;

    let gap = material.geometry.gap;
    let reach = material.geometry.mark_size;
    let heavy = stroke(q, HEAVY_DIR, gap, reach);
    let light = stroke(q, LIGHT_DIR, gap, reach);

    let aa = fwidth(heavy);
    let ink = max(
        1.0 - smoothstep(material.geometry.heavy_width * 0.5 - aa, material.geometry.heavy_width * 0.5 + aa, heavy),
        1.0 - smoothstep(material.geometry.light_width * 0.5 - aa, material.geometry.light_width * 0.5 + aa, light),
    );

    let mark_rgb = mix(material.mark_color.rgb, material.hover_color.rgb, hover);
    color += mark_rgb * ink;
    alpha = max(alpha, ink * material.mark_color.a);

    if (alpha < 0.002) {
        return vec4<f32>(0.0);
    }
    return vec4<f32>(color, alpha);
}
