#import bevy_ui::ui_vertex_output::{UiVertexOutput}
#import bevy_render::globals::{Globals}

// Raked progress bar with three positions: completed progress, the resource-reachable marker,
// and a lagging band between them. The runway closes when progress reaches the marker.
//
// Track-space `along` measures horizontal distance from the raked start edge, reducing all
// position comparisons to one dimension while keeping boundaries parallel to the ends.

@group(0) @binding(1)
var<uniform> globals: Globals;

// Must stay identical to `Fade` in the other UI shaders and on the Rust side.
struct Fade {
    start_value: f32,
    end_value: f32,
    start_time: f32,
    rate: f32,
};

struct ProgressBarGeometry {
    // Horizontal shift of the top edge relative to the bottom, as a fraction of the track's
    // height. 0 is a plain rectangle; 1 rakes it by its full height.
    rake: f32,
    rail_thickness: f32,
    edge_width: f32,
    marker_width: f32,
};

struct ProgressBarShading {
    // How far the glow behind the progress edge reaches back, in pixels.
    edge_falloff: f32,
    // Bottom-of-track brightness of the earned fill; the top gets the same amount less, so
    // the fill has weight and reads as substance rather than as a percentage.
    fill_gradient: f32,
    // Brightness of the runway relative to the earned fill.
    runway_gain: f32,
    // Brightness of the track's interior beyond the marker.
    inert_gain: f32,
};

struct ProgressBarDetail {
    // Graduation marks: how far they reach in from a rail, and how wide they are.
    tick_length: f32,
    tick_width: f32,
    tick_gain: f32,
    // Runway width in pixels at or below which the progress edge counts as stalled. A few
    // pixels rather than zero, so the two lines do not have to land on the same pixel.
    stall_gap: f32,
};

struct ProgressBarMaterial {
    fill_color: vec4<f32>,
    edge_color: vec4<f32>,
    track_color: vec4<f32>,
    rail_color: vec4<f32>,
    marker_color: vec4<f32>,
    stall_color: vec4<f32>,
    geometry: ProgressBarGeometry,
    shading: ProgressBarShading,
    detail: ProgressBarDetail,
    progress_fade: Fade,
    marker_fade: Fade,
    band_fade: Fade,
};

@group(1) @binding(0)
var<uniform> material: ProgressBarMaterial;

// Floor the runway's fade-out, so a long runway stays visible to its end rather than
// dissolving into the track.
const RUNWAY_FLOOR: f32 = 0.4;

// Graduations, as fractions of the track. Fixed quarters: a scale to read against, not a
// count of anything.
const TICK_A: f32 = 0.25;
const TICK_B: f32 = 0.5;
const TICK_C: f32 = 0.75;

fn eased(fade: Fade, now: f32) -> f32 {
    let elapsed = max(0.0, now - fade.start_time);
    return mix(fade.start_value, fade.end_value, 1.0 - exp(-fade.rate * elapsed));
}

@fragment
fn fragment(in: UiVertexOutput) -> @location(0) vec4<f32> {
    let size = in.size;
    let x = in.uv.x * size.x;
    let y = in.uv.y * size.y;
    let half_height = size.y * 0.5;

    // ---- Track space ----
    // The rake shifts the top edge right of the bottom, so the track leans in the direction
    // it fills. `body` is what is left of the width once both ends have room to lean.
    let reach = material.geometry.rake * half_height;
    let body = max(size.x - 2.0 * reach, 1.0);
    let start_x = reach + material.geometry.rake * (half_height - y);
    let along = x - start_x;

    let from_side = abs(along - body * 0.5) - body * 0.5;
    let from_rail = abs(y - half_height) - (half_height - 1.0);
    let d = max(from_side, from_rail);
    let aa = fwidth(d) * 0.7;
    let coverage = 1.0 - smoothstep(-aa, aa, d);
    if (coverage < 0.001) {
        return vec4<f32>(0.0);
    }

    // ---- The three positions, in pixels along the track ----
    let progress_x = eased(material.progress_fade, globals.time) * body;
    let band_x = eased(material.band_fade, globals.time) * body;
    let marker_x = eased(material.marker_fade, globals.time) * body;

    let behind_progress = progress_x - along;
    let earned = smoothstep(-1.0, 1.0, behind_progress);
    let within_band = smoothstep(-1.0, 1.0, band_x - along);
    let within_marker = smoothstep(-1.0, 1.0, marker_x - along);

    // ---- Interior ----
    var color = material.track_color.rgb * mix(material.shading.inert_gain, 1.0, within_marker);

    // ---- Runway ----
    // Fade from the progress edge toward the marker.
    let runway = max(within_band - earned, 0.0);
    let runway_fade = mix(
        RUNWAY_FLOOR,
        1.0,
        exp(-max(along - progress_x, 0.0) / material.shading.edge_falloff),
    );
    color += material.fill_color.rgb * material.shading.runway_gain * runway_fade * runway;

    // ---- Earned fill ----
    let weight = mix(2.0 - material.shading.fill_gradient, material.shading.fill_gradient, in.uv.y);
    let glow = exp(-max(behind_progress, 0.0) / material.shading.edge_falloff);
    color += material.fill_color.rgb * weight * mix(0.75, 1.0, glow) * earned;

    // ---- Rails and graduations ----
    // The rails run the whole length and dim past the marker along with the interior. The
    // graduations do not dim at all: they are the scale the track is read against, and a
    // scale that disappears where you cannot afford to go is a scale you might never see.
    let rail = 1.0 - smoothstep(
        -material.geometry.rail_thickness - aa,
        -material.geometry.rail_thickness + aa,
        from_rail,
    );
    color += material.rail_color.rgb * rail * mix(material.shading.inert_gain, 1.0, within_marker);

    // A graduation hangs off a rail instead of crossing the interior, so it reads as part of
    // the scale rather than as a value on it.
    let near_rail = 1.0 - smoothstep(
        -material.detail.tick_length - aa,
        -material.detail.tick_length + aa,
        from_rail,
    );
    let nearest_tick = min(
        abs(along - TICK_B * body),
        min(abs(along - TICK_A * body), abs(along - TICK_C * body)),
    );
    let tick = 1.0 - smoothstep(
        material.detail.tick_width * 0.5 - 0.5,
        material.detail.tick_width * 0.5 + 0.5,
        nearest_tick,
    );
    color += material.rail_color.rgb * material.detail.tick_gain * tick * near_rail;

    // ---- Progress edge ----
    // The bright head of the fill, and the only thing that carries the stall colour: stalling
    // is the head arriving at the marker, so the colour belongs to the collision rather than
    // to the wall it collides with.
    let stalled = step(marker_x - progress_x, material.detail.stall_gap);
    let edge = 1.0 - smoothstep(
        material.geometry.edge_width * 0.5 - 0.5,
        material.geometry.edge_width * 0.5 + 0.5,
        abs(along - progress_x),
    );
    color += mix(material.edge_color.rgb, material.stall_color.rgb, stalled) * edge;

    // ---- Marker ----
    // Yields wherever the progress edge is drawn. The two sit on top of each other at a
    // stall, and adding them there would wash the stall colour back out toward white.
    let marker = (1.0 - smoothstep(
        material.geometry.marker_width * 0.5 - 0.5,
        material.geometry.marker_width * 0.5 + 0.5,
        abs(along - marker_x),
    )) * (1.0 - edge);
    color += material.marker_color.rgb * marker;

    return vec4<f32>(color, coverage);
}
