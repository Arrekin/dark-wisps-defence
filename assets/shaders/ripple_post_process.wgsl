#import bevy_core_pipeline::fullscreen_vertex_shader::FullscreenVertexOutput

@group(0) @binding(0) var screen_texture: texture_2d<f32>;
@group(0) @binding(1) var screen_sampler: sampler;
@group(0) @binding(2) var<uniform> camera: CameraData;
@group(0) @binding(3) var<storage, read> ripples: array<RippleEntry>;

struct CameraData {
    world_pos: vec2<f32>,
    viewport_size: vec2<f32>,
    ripple_count: u32,
}

struct RippleEntry {
    center: vec2<f32>,
    normalized_radius: f32,
    max_radius: f32,
}

// Effect switches (compile-time).
const LEADING_BRIGHTNESS: bool  = true;
const COLOR_BOOST: bool         = true;
const CHROMATIC_ABERRATION: bool = false;

// Tunables.
const WAVE_WIDTH: f32       = 0.15;
const BOOST_INTENSITY: f32  = 0.22;
const MAX_DISPLACEMENT: f32 = 15.0;
const PI: f32               = 3.14159265;

fn uv_to_world(uv: vec2<f32>) -> vec2<f32> {
    let centered = uv - vec2<f32>(0.5, 0.5);
    return camera.world_pos + centered * camera.viewport_size * vec2<f32>(1.0, -1.0);
}

fn world_to_uv(world: vec2<f32>) -> vec2<f32> {
    let centered = (world - camera.world_pos) / camera.viewport_size;
    return centered * vec2<f32>(1.0, -1.0) + vec2<f32>(0.5, 0.5);
}

fn apply_color_boost(rgb: vec3<f32>, strength: f32) -> vec3<f32> {
    if !COLOR_BOOST || strength <= 0.0 {
        return rgb;
    }
    return rgb * (1.0 + strength * BOOST_INTENSITY);
}

@fragment
fn fragment(in: FullscreenVertexOutput) -> @location(0) vec4<f32> {
    let count = camera.ripple_count;

    if count == 0u {
        return textureSampleLevel(screen_texture, screen_sampler, in.uv, 0.0);
    }

    let world_pos = uv_to_world(in.uv);

    var total_displacement   = vec2<f32>(0.0);
    var total_displacement_r = vec2<f32>(0.0);
    var total_displacement_b = vec2<f32>(0.0);
    var brightness_factor    = 1.0;
    var strength             = 0.0;

    for (var i = 0u; i < count; i++) {
        let entry       = ripples[i];
        let diameter    = entry.max_radius * 2.0;
        let dist_world  = distance(world_pos, entry.center);
        let dist        = dist_world / diameter;

        let envelope    = sin(entry.normalized_radius * 2.0 * PI);
        let wave_width  = max(WAVE_WIDTH * envelope, 0.001);

        if dist < (entry.normalized_radius - wave_width) || dist > entry.normalized_radius {
            continue;
        }

        let band_t     = (dist - (entry.normalized_radius - wave_width)) / wave_width;
        let wave_shape = sin(band_t * PI);
        let amplitude  = wave_shape * MAX_DISPLACEMENT * envelope;
        let radial_dir = normalize(world_pos - entry.center);

        total_displacement += radial_dir * amplitude;

        if CHROMATIC_ABERRATION {
            let shift = 0.8 * wave_shape * envelope;
            total_displacement_r += radial_dir * (amplitude + shift * MAX_DISPLACEMENT);
            total_displacement_b += radial_dir * (amplitude - shift * MAX_DISPLACEMENT);
        }

        if LEADING_BRIGHTNESS {
            let leading_t = max(0.0, band_t * 2.0 - 1.0);
            brightness_factor *= 1.0 + 1.2 * leading_t * leading_t * envelope;
        }

        if COLOR_BOOST {
            strength += wave_shape * envelope;
        }
    }

    let displaced_uv = world_to_uv(world_pos - total_displacement);

    var rgb: vec3<f32>;
    if CHROMATIC_ABERRATION {
        let uv_r = world_to_uv(world_pos - total_displacement_r);
        let uv_b = world_to_uv(world_pos - total_displacement_b);
        rgb = vec3<f32>(
            textureSampleLevel(screen_texture, screen_sampler, uv_r, 0.0).r,
            textureSampleLevel(screen_texture, screen_sampler, displaced_uv, 0.0).g,
            textureSampleLevel(screen_texture, screen_sampler, uv_b, 0.0).b,
        );
    } else {
        rgb = textureSampleLevel(screen_texture, screen_sampler, displaced_uv, 0.0).rgb;
    }

    rgb *= brightness_factor;
    rgb = apply_color_boost(rgb, strength);

    return vec4<f32>(rgb, 1.0);
}
