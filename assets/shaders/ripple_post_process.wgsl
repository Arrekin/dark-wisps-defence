#import bevy_core_pipeline::fullscreen_vertex_shader::FullscreenVertexOutput

@group(0) @binding(0) var screen_texture: texture_2d<f32>;
@group(0) @binding(1) var screen_sampler: sampler;
@group(0) @binding(2) var<uniform> post_process: RipplePostProcess;

struct GpuRippleEntry {
    // xy = world center, z = normalized_radius (0..0.5), w = mesh_half_size (world units)
    data0: vec4<f32>,
    // x = wave_width, yzw = padding
    data1: vec4<f32>,
}

struct RipplePostProcess {
    camera_world_pos: vec2<f32>,
    viewport_world_size: vec2<f32>,
    // x = active ripple count; yzw = padding
    count_and_pad: vec4<u32>,
    entries: array<GpuRippleEntry, 32>,
}

const LEADING_BRIGHTNESS: bool   = true;
const EMISSIVE_TINT: bool        = true;
const CHROMATIC_ABERRATION: bool = false;

const PI: f32               = 3.14159265;
const MAX_DISPLACEMENT: f32 = 15.0;

// Convert screen UV (0..1, Y=0 at top) to world XY.
fn uv_to_world(uv: vec2<f32>) -> vec2<f32> {
    let centered = uv - vec2<f32>(0.5, 0.5);
    return post_process.camera_world_pos + centered * post_process.viewport_world_size * vec2<f32>(1.0, -1.0);
}

// Convert world XY back to screen UV.
fn world_to_uv(world: vec2<f32>) -> vec2<f32> {
    let centered = (world - post_process.camera_world_pos) / post_process.viewport_world_size;
    return centered * vec2<f32>(1.0, -1.0) + vec2<f32>(0.5, 0.5);
}

@fragment
fn fragment(in: FullscreenVertexOutput) -> @location(0) vec4<f32> {
    let count = post_process.count_and_pad.x;

    if count == 0u {
        return textureSampleLevel(screen_texture, screen_sampler, in.uv, 0.0);
    }

    let world_pos = uv_to_world(in.uv);

    var total_displacement   = vec2<f32>(0.0);
    var total_displacement_r = vec2<f32>(0.0);
    var total_displacement_b = vec2<f32>(0.0);
    var brightness_factor    = 1.0;
    var emissive             = vec3<f32>(0.0);

    for (var i = 0u; i < count; i++) {
        let entry        = post_process.entries[i];
        let center       = entry.data0.xy;
        let norm_radius  = entry.data0.z;
        let half_size    = entry.data0.w;
        let wave_width_n = entry.data1.x;

        let dist_world = distance(world_pos, center);
        let dist       = dist_world / (half_size * 2.0);

        let envelope   = sin(norm_radius * 2.0 * PI);
        let wave_width = max(wave_width_n * envelope, 0.001);

        if dist < (norm_radius - wave_width) || dist > norm_radius {
            continue;
        }

        let band_t     = (dist - (norm_radius - wave_width)) / wave_width;
        let wave_shape = sin(band_t * PI);
        let amplitude  = wave_shape * MAX_DISPLACEMENT * envelope;
        let radial_dir = normalize(world_pos - center);

        total_displacement += radial_dir * amplitude;

        if CHROMATIC_ABERRATION {
            let shift = 0.8 * wave_shape * envelope;
            total_displacement_r += radial_dir * (amplitude + shift * MAX_DISPLACEMENT);
            total_displacement_b += radial_dir * (amplitude - shift * MAX_DISPLACEMENT);
        }

        if LEADING_BRIGHTNESS {
            let leading_t = max(0.0, band_t * 2.0 - 1.0);
            brightness_factor *= 1.0 + 0.6 * leading_t * leading_t * envelope;
        }

        if EMISSIVE_TINT {
            emissive += vec3<f32>(0.2, 0.8, 1.0) * (wave_shape * envelope * 0.025);
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

    return vec4<f32>(rgb * brightness_factor + emissive, 1.0);
}
