#import bevy_sprite::mesh2d_vertex_output::VertexOutput
#import bevy_sprite::mesh2d_view_bindings::view

@group(2) @binding(0) var scene_texture: texture_2d<f32>;
@group(2) @binding(1) var scene_sampler: sampler;

struct RippleData {
    current_radius: f32,
    wave_width: f32,
};

@group(2) @binding(2) var<uniform> uniforms: RippleData;

const PI: f32                = 3.14159265;
// Maximum radial displacement in world units, applied at the wave's peak.
const MAX_DISPLACEMENT: f32  = 15.0;
// Set > 0.0 to overlay a white marker line at the leading and trailing band edges.
const EDGE_LINE_OPACITY: f32 = 0.0;

@fragment
fn fragment(mesh: VertexOutput) -> @location(0) vec4<f32> {
    let center = vec2<f32>(0.5, 0.5);
    let dist   = distance(mesh.uv, center);

    let current_radius = uniforms.current_radius;

    // Lifetime envelope: maps the 0→0.5 normalised lifetime to a smooth bell
    // (0 at birth, peak at mid-travel, 0 at death). Scales both wave_width and
    // displacement amplitude so the band grows, holds, then collapses cleanly.
    let envelope   = sin(current_radius * 2.0 * PI);
    let wave_width = max(uniforms.wave_width * envelope, 0.001);

    // Outside the band: transparent, lets the underlying render show through.
    // AlphaMode2d::Blend means multiple overlapping ripples compose independently.
    if dist < (current_radius - wave_width) || dist > current_radius {
        return vec4<f32>(0.0, 0.0, 0.0, 0.0);
    }

    // band_t: 0 at trailing (inner) edge, 1 at leading (outer) edge.
    let band_t     = (dist - (current_radius - wave_width)) / wave_width;
    // Half-sine: zero at both edges, single peak at centre.
    // Produces one smooth displacement bump with no double-ring artifact.
    let wave_shape = sin(band_t * PI);
    let amplitude  = wave_shape * MAX_DISPLACEMENT * envelope;

    // Radial direction: UV Y is down, world Y is up, so flip Y component.
    let uv_dir     = mesh.uv - center;
    let radial_dir = normalize(vec2<f32>(uv_dir.x, -uv_dir.y));

    // Inward sampling: displacing the sample point toward the centre means
    // scene content appears to be pushed outward by the passing wave.
    let displaced_world = mesh.world_position - vec4<f32>(radial_dir * amplitude, 0.0, 0.0);
    let displaced_clip  = view.clip_from_world * displaced_world;
    let displaced_ndc   = displaced_clip.xy / displaced_clip.w;
    let displaced_uv    = displaced_ndc * vec2<f32>(0.5, -0.5) + 0.5;

    var rgb = textureSample(scene_texture, scene_sampler, displaced_uv).rgb;

    // Optional 1px debug lines at the band edges. fwidth gives ~1 screen-pixel
    // width in UV space so the lines stay 1px regardless of zoom level.
    if EDGE_LINE_OPACITY > 0.0 {
        let px          = fwidth(dist);
        let at_leading  = dist >= current_radius - px && dist <= current_radius;
        let at_trailing = dist >= (current_radius - wave_width) && dist <= (current_radius - wave_width + px);
        if at_leading || at_trailing {
            rgb = mix(rgb, vec3<f32>(1.0, 1.0, 1.0), EDGE_LINE_OPACITY);
        }
    }

    return vec4<f32>(rgb, 1.0);
}
