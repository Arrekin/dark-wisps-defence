#import bevy_sprite::mesh2d_vertex_output::VertexOutput
#import bevy_sprite::mesh2d_view_bindings::view

@group(2) @binding(0) var scene_texture: texture_2d<f32>;
@group(2) @binding(1) var scene_sampler: sampler;

struct RippleData {
    current_radius: f32,
    wave_width: f32,
};

@group(2) @binding(2) var<uniform> uniforms: RippleData;

const PI: f32               = 3.14159265;
const MAX_DISPLACEMENT: f32 = 15.0;

// ── Effect toggles ─────────────────────────────────────────────────────────────
// Each effect is independent and can be combined freely.
const LEADING_BRIGHTNESS: bool  = true;  // brightens the leading half of the band
const EMISSIVE_TINT: bool       = true;  // additive cyan glow following the wave bell
const CHROMATIC_ABERRATION: bool = false; // splits R/G/B channels radially
const TRAILING_DISSOLUTION: bool = false; // dithers the trailing edge out
// ──────────────────────────────────────────────────────────────────────────────

@fragment
fn fragment(mesh: VertexOutput) -> @location(0) vec4<f32> {
    let center = vec2<f32>(0.5, 0.5);
    let dist   = distance(mesh.uv, center);

    let current_radius = uniforms.current_radius;

    // Lifetime envelope: sin bell over the 0→0.5 normalised lifetime.
    // Scales wave_width and amplitude so the band is born and dies smoothly.
    let envelope   = sin(current_radius * 2.0 * PI);
    let wave_width = max(uniforms.wave_width * envelope, 0.001);

    // Outside the band: transparent, lets the underlying render show through.
    if dist < (current_radius - wave_width) || dist > current_radius {
        return vec4<f32>(0.0, 0.0, 0.0, 0.0);
    }

    // band_t: 0 at trailing edge, 1 at leading edge.
    let band_t     = (dist - (current_radius - wave_width)) / wave_width;
    // Half-sine profile: zero at both edges, peak at centre.
    let wave_shape = sin(band_t * PI);
    let amplitude  = wave_shape * MAX_DISPLACEMENT * envelope;

    // Radial direction: UV Y is down, world Y is up, so flip Y component.
    let uv_dir     = mesh.uv - center;
    let radial_dir = normalize(vec2<f32>(uv_dir.x, -uv_dir.y));

    // Inward sampling: sample point moves toward centre, content appears pushed outward.
    let displaced_world = mesh.world_position - vec4<f32>(radial_dir * amplitude, 0.0, 0.0);
    let displaced_clip  = view.clip_from_world * displaced_world;
    let displaced_ndc   = displaced_clip.xy / displaced_clip.w;
    let displaced_uv    = displaced_ndc * vec2<f32>(0.5, -0.5) + 0.5;

    var rgb = textureSample(scene_texture, scene_sampler, displaced_uv).rgb;

    // ── Chromatic aberration ───────────────────────────────────────────────────
    // R sampled further out, B further in — colour fringing makes wave geometry legible.
    if CHROMATIC_ABERRATION {
        let shift   = 0.8 * wave_shape * envelope;
        let world_r = mesh.world_position - vec4<f32>(radial_dir * (amplitude + shift * MAX_DISPLACEMENT), 0.0, 0.0);
        let world_b = mesh.world_position - vec4<f32>(radial_dir * (amplitude - shift * MAX_DISPLACEMENT), 0.0, 0.0);
        let clip_r  = view.clip_from_world * world_r;
        let clip_b  = view.clip_from_world * world_b;
        let uv_r    = (clip_r.xy / clip_r.w) * vec2<f32>(0.5, -0.5) + 0.5;
        let uv_b    = (clip_b.xy / clip_b.w) * vec2<f32>(0.5, -0.5) + 0.5;
        rgb = vec3<f32>(
            textureSample(scene_texture, scene_sampler, uv_r).r,
            rgb.g,
            textureSample(scene_texture, scene_sampler, uv_b).b,
        );
    }

    // ── Leading-edge brightness flash ─────────────────────────────────────────
    // Boosts luminance at the wavefront (band_t near 1.0), mimicking a pressure front.
    if LEADING_BRIGHTNESS {
        let leading_t  = max(0.0, band_t * 2.0 - 1.0);
        let brightness = 1.0 + 0.6 * leading_t * leading_t * envelope;
        rgb = rgb * brightness;
    }

    // ── Emissive tint ─────────────────────────────────────────────────────────
    // Additive cyan glow proportional to wave_shape; visible even on dark backgrounds.
    if EMISSIVE_TINT {
        rgb = rgb + vec3<f32>(0.2, 0.8, 1.0) * (wave_shape * envelope * 0.025);
    }

    // ── Trailing-edge dissolution ──────────────────────────────────────────────
    // Dithers the trailing edge out via 4x4 Bayer ordered dithering.
    if TRAILING_DISSOLUTION {
        let bayer4 = array<f32, 16>(
             0.0/16.0,  8.0/16.0,  2.0/16.0, 10.0/16.0,
            12.0/16.0,  4.0/16.0, 14.0/16.0,  6.0/16.0,
             3.0/16.0, 11.0/16.0,  1.0/16.0,  9.0/16.0,
            15.0/16.0,  7.0/16.0, 13.0/16.0,  5.0/16.0,
        );
        let px_coord = vec2<u32>(mesh.position.xy) % vec2<u32>(4u, 4u);
        if band_t < bayer4[px_coord.y * 4u + px_coord.x] {
            return vec4<f32>(0.0, 0.0, 0.0, 0.0);
        }
    }

    return vec4<f32>(rgb, 1.0);
}
