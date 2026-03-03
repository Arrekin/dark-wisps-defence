// Ring displacement shader — treats the ripple band as a 3D cylindrical tube.
//
// Computes a fake surface normal from band position and applies directional
// diffuse lighting, giving the flat displacement ring the appearance of a
// physical ring with depth and shading.
//
// Drop-in replacement for ripple.wgsl — same bindings, same RippleData layout.

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
// CYLINDRICAL_SHADING  Directional diffuse on the ring tube's cylinder normal.
//                      Inner face shadowed, outer face lit, top brightest.
//                      Ambient 0.35 keeps it readable throughout lifetime.
// SPECULAR_HIGHLIGHT   Blinn-Phong specular arc on the same normal.
// RIM_GLOW             Additive emissive at band edges (band_t ≈ 0 and 1).
// DITHERED_SHADING     At shadowed inner-edge pixels, Bayer-dithers between the
//                      shaded and raw displaced sample. Top (normal_up ≈ 1)
//                      is never dithered.
// FADE_AT_DEATH        Second half of life: smooth alpha fade across the whole band.
//                      Plays cleanly with DITHERED_SHADING.
const CYLINDRICAL_SHADING: bool = true;
const SPECULAR_HIGHLIGHT: bool  = false;
const RIM_GLOW: bool            = false;
const DITHERED_SHADING: bool    = true;
const FADE_AT_DEATH: bool       = true;
// ──────────────────────────────────────────────────────────────────────────────

@fragment
fn fragment(mesh: VertexOutput) -> @location(0) vec4<f32> {
    let center         = vec2<f32>(0.5, 0.5);
    let dist           = distance(mesh.uv, center);
    let current_radius = uniforms.current_radius;

    let envelope       = sin(current_radius * 2.0 * PI);
    // Width uses only the birth ramp so it locks at max from mid-life onward.
    // This prevents the inner edge from chasing the outer as the ring dies.
    let birth_envelope = clamp(current_radius * 4.0, 0.0, 1.0);
    let wave_width     = max(uniforms.wave_width * birth_envelope, 0.001);

    if dist < (current_radius - wave_width) || dist > current_radius {
        return vec4<f32>(0.0, 0.0, 0.0, 0.0);
    }

    // band_t: 0 at trailing edge, 1 at leading edge.
    let band_t     = (dist - (current_radius - wave_width)) / wave_width;
    let wave_shape = sin(band_t * PI);
    let amplitude  = wave_shape * MAX_DISPLACEMENT * envelope;

    let uv_dir     = mesh.uv - center;
    let radial_dir = normalize(vec2<f32>(uv_dir.x, -uv_dir.y));

    let displaced_world = mesh.world_position - vec4<f32>(radial_dir * amplitude, 0.0, 0.0);
    let displaced_clip  = view.clip_from_world * displaced_world;
    let displaced_ndc   = displaced_clip.xy / displaced_clip.w;
    let displaced_uv    = displaced_ndc * vec2<f32>(0.5, -0.5) + 0.5;

    var rgb = textureSample(scene_texture, scene_sampler, displaced_uv).rgb;
    let rgb_displaced = rgb;

    // Cylinder surface normal: inner face (band_t=0) points inward, top (0.5) faces
    // the camera, outer face (band_t=1) points outward.
    let normal_radial  = -cos(band_t * PI);
    let normal_up      = wave_shape;  // sin(band_t * PI) — reuses already-computed value
    let surface_normal = normalize(vec3<f32>(
        normal_radial * radial_dir.x,
        normal_radial * radial_dir.y,
        normal_up,
    ));

    // Light from upper-left at ~45° elevation — fixed in screen space.
    let light_dir = normalize(vec3<f32>(-0.5, 0.5, 1.2));
    let diffuse   = max(0.0, dot(surface_normal, light_dir));

    // ── Cylindrical shading ────────────────────────────────────────────────────
    // Ambient 0.35 keeps the ring readable even as amplitude fades at end of life.
    if CYLINDRICAL_SHADING {
        rgb = rgb * (0.35 + 0.65 * diffuse);
    }

    // ── Dithered shading ──────────────────────────────────────────────────────
    // At shadowed inner-edge pixels, Bayer-dithers between the shaded value and the
    // raw displaced sample, softening the shadow boundary with a halftone pattern.
    // Density peaks where diffuse is low and band_t is near 0 (inner edge).
    // At the top (normal_up = 1) edge_weight = 0, so no dithering there.
    if DITHERED_SHADING {
        let bayer4 = array<f32, 16>(
             0.0/16.0,  8.0/16.0,  2.0/16.0, 10.0/16.0,
            12.0/16.0,  4.0/16.0, 14.0/16.0,  6.0/16.0,
             3.0/16.0, 11.0/16.0,  1.0/16.0,  9.0/16.0,
            15.0/16.0,  7.0/16.0, 13.0/16.0,  5.0/16.0,
        );
        let px_coord      = vec2<u32>(mesh.position.xy) % vec2<u32>(4u, 4u);
        let bayer_val     = bayer4[px_coord.y * 4u + px_coord.x];
        let edge_weight   = (1.0 - normal_up) * (1.0 - band_t); // 1 at inner edge, 0 at outer and top
        let shadow_weight = max(0.0, 1.0 - diffuse * 2.0);      // 1 in deep shadow, 0 where lit
        let dither_zone   = edge_weight * shadow_weight;
        if dither_zone > bayer_val {
            rgb = mix(rgb, rgb_displaced, shadow_weight);
        }
    }

    // ── Specular highlight ─────────────────────────────────────────────────────
    if SPECULAR_HIGHLIGHT {
        let half_vec = normalize(light_dir + vec3<f32>(0.0, 0.0, 1.0));
        let spec     = pow(max(0.0, dot(surface_normal, half_vec)), 48.0);
        rgb = rgb + vec3<f32>(0.9, 0.95, 1.0) * spec * 0.2 * envelope;
    }

    // ── Rim glow ──────────────────────────────────────────────────────────────
    if RIM_GLOW {
        let rim = 1.0 - wave_shape;
        rgb = rgb + vec3<f32>(0.2, 0.7, 1.0) * rim * rim * 0.5 * envelope;
    }

    // ── Fade at death ─────────────────────────────────────────────────────────
    // In the second half of life, fades the whole band uniformly so both edges
    // dissolve together rather than the inner edge chasing the outer.
    if FADE_AT_DEATH {
        let death_progress = clamp((current_radius - 0.25) * 4.0, 0.0, 1.0);
        if death_progress > 0.0 {
            let fade_alpha = clamp(1.0 - death_progress * death_progress, 0.0, 1.0);
            return vec4<f32>(rgb, fade_alpha);
        }
    }

    return vec4<f32>(rgb, 1.0);
}
