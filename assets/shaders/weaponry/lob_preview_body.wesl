import bevy_sprite_render::mesh2d::vertex_output::VertexOutput;
import package::shaders::common::color::display_to_linear;
import package::shaders::visuals::drop_shadow_cast::disc;
import package::shaders::weaponry::lob_preview_dots::{arc_height, dot_gaps, dot_presence, dot_progress, last_dot};

// Airborne dot trail for a lob preview.
//
// Perspective scaling enlarges and brightens dots with height and shifts them outward from the
// chord midpoint along the `origin -> landing` line, compressing dot spacing near launch and
// landing while keeping the projected path colinear with the ground chord.

// Field order and types mirror `LobPreviewBody` in weaponry_internal/src/lob_preview_paths.rs.
struct Instance {
    origin: vec2<f32>,
    landing: vec2<f32>,
    apex: f32,  // peak height above ground, in world units
};

// ── Perspective ──────────────────────────────────────────────────────────────
// Mirrored by `CAMERA_HEIGHT` and `MAX_VISUAL_HEIGHT` in weaponry_internal/src/lob.rs.
const CAMERA_HEIGHT: f32 = 320.0;
const MAX_VISUAL_HEIGHT: f32 = 0.85;
const END_GAP: f32 = 0.15;  // endpoint dot spacing as a fraction of uniform ground spacing

// ── Dots ─────────────────────────────────────────────────────────────────────
// Mirrored by `BODY_DOT_DIAMETER` in weaponry_internal/src/lob_preview_paths.rs.
const DOT_DIAMETER: f32 = 4.0;  // diameter at ground level, in world units
const ALPHA_LOW: f32 = 0.35;    // opacity at ground level (`scale = 1`)
const ALPHA_HIGH: f32 = 0.8;    // opacity at `scale >= 2`
const COLOR: vec3<f32> = vec3<f32>(1.0, 0.7, 0.4);  // warm orange, display space

fn look(mesh: VertexOutput, path: Instance) -> vec4<f32> {
    let p = mesh.world_position.xy;
    let midpoint = 0.5 * (path.origin + path.landing);
    let visual_apex = clamp(path.apex, 1.0, CAMERA_HEIGHT * MAX_VISUAL_HEIGHT);
    // Endpoint screen speed / ground speed = 1 - 2 * stretch * apex / CAMERA_HEIGHT.
    let stretch = (1.0 - END_GAP) * CAMERA_HEIGHT / (2.0 * visual_apex);
    let gaps = dot_gaps(path.origin, path.landing);

    var alpha = 0.0;
    for (var index = 0u; index <= last_dot(gaps); index++) {
        let progress = dot_progress(index, gaps);
        let presence = dot_presence(progress);
        if presence <= 0.0 { continue; }
        let height = min(arc_height(path.apex, progress), CAMERA_HEIGHT * MAX_VISUAL_HEIGHT);
        let scale = CAMERA_HEIGHT / (CAMERA_HEIGHT - height);
        let ground = mix(path.origin, path.landing, progress);
        let centre = ground + (ground - midpoint) * stretch * (scale - 1.0);
        let coverage = disc(length(p - centre) / (0.5 * DOT_DIAMETER * scale));
        let dot_alpha = mix(ALPHA_LOW, ALPHA_HIGH, min(scale - 1.0, 1.0));
        alpha = max(alpha, coverage * dot_alpha * presence);
    }
    return vec4<f32>(display_to_linear(COLOR), alpha);
}
