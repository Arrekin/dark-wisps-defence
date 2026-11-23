import bevy_sprite_render::mesh2d::vertex_output::VertexOutput;
import package::shaders::visuals::drop_shadow_cast::{cast_alpha, surface_rise};
import package::shaders::weaponry::lob_preview_dots::{arc_height, dot_gaps, dot_presence, dot_progress, last_dot};

// Ground-projected shadow dot trail for a lob preview.
//
// Casts each arc dot through `drop_shadow_cast` using `landing` as the elevation reference cell so
// the shadow trail converges with the body trail at the landing point.

// Field order and types mirror `LobPreviewShadow` in weaponry_internal/src/lob_preview_paths.rs.
struct Instance {
    origin: vec2<f32>,
    landing: vec2<f32>,  // landing position and reference cell for surface elevation
    apex: f32,           // peak height above ground, in world units
};

// Mirrored by `SHADOW_DOT_DIAMETER` in weaponry_internal/src/lob_preview_paths.rs.
const DOT_DIAMETER: f32 = 6.0;  // diameter at zero height, in world units
const OPACITY: f32 = 0.6;       // opacity multiplier relative to `SHADOW_ALPHA`

fn look(mesh: VertexOutput, path: Instance) -> vec4<f32> {
    let p = mesh.world_position.xy;
    let rise = surface_rise(p, path.landing);
    let gaps = dot_gaps(path.origin, path.landing);

    var alpha = 0.0;
    for (var index = 0u; index <= last_dot(gaps); index++) {
        let progress = dot_progress(index, gaps);
        let presence = dot_presence(progress);
        if presence <= 0.0 { continue; }
        let ground = mix(path.origin, path.landing, progress);
        let height = arc_height(path.apex, progress);
        alpha = max(alpha, cast_alpha(p, ground, rise, height, DOT_DIAMETER) * presence);
    }
    return vec4<f32>(0.0, 0.0, 0.0, OPACITY * alpha);
}
