#import bevy_sprite::mesh2d_vertex_output::VertexOutput
#import bevy_sprite::mesh2d_view_bindings::globals

struct UniformData {
    radiance_angle: f32, // 10.0 - 30.0
    radiance_radius: f32, // 5.0 - 15.0
};

@group(2) @binding(4)
var<uniform> uniforms: UniformData;

const WISP_EFFECT_SLOTS: u32 = 8u;
struct WispEffects {
    mask: u32,
    params: array<vec4<f32>, WISP_EFFECT_SLOTS>,
};
@group(2) @binding(5)
var<uniform> effects: WispEffects;

const BRITTLE: u32 = 1u;

// Hash function to generate pseudo-random values
fn hash(p: vec2<f32>) -> f32 {
    let h: f32 = dot(p, vec2<f32>(127.1, 311.7));
    return fract(sin(h) * 43758.5453123);
}

// Noise function using the hash function
fn noise(p: vec2<f32>) -> f32 {
    let i: vec2<f32> = floor(p);
    let f: vec2<f32> = fract(p);
    let a: f32 = hash(i);
    let b: f32 = hash(i + vec2<f32>(1.0, 0.0));
    let c: f32 = hash(i + vec2<f32>(0.0, 1.0));
    let d: f32 = hash(i + vec2<f32>(1.0, 1.0));
    let u: vec2<f32> = f * f * (3.0 - 2.0 * f);
    return mix(mix(a, b, u.x), mix(c, d, u.x), u.y);
}

// ── Procedural brittle cracks ───────────────────────────────────────────────
// A cellular/Worley crack web that fractures the radiance like cracked glass — thin dark
// fissures where the light can no longer hold together.
const CRACK_SCALE: f32 = 4.0;
const CRACK_WIDTH: f32 = 0.20;
const CRACK_STRENGTH: f32 = 1.0;

fn crack_hash2(p: vec2<f32>) -> vec2<f32> {
    let k = vec2<f32>(
        dot(p, vec2<f32>(127.1, 311.7)),
        dot(p, vec2<f32>(269.5, 183.3)),
    );
    return fract(sin(k) * 43758.5453123);
}

fn crack_worley(uv: vec2<f32>) -> vec2<f32> {
    let g = floor(uv);
    let f = fract(uv);
    var f1: f32 = 8.0;
    var f2: f32 = 8.0;
    for (var j: i32 = -1; j <= 1; j = j + 1) {
        for (var i: i32 = -1; i <= 1; i = i + 1) {
            let cell = vec2<f32>(f32(i), f32(j));
            let point = cell + crack_hash2(g + cell);
            let d = length(point - f);
            if (d < f1) {
                f2 = f1;
                f1 = d;
            } else if (d < f2) {
                f2 = d;
            }
        }
    }
    return vec2<f32>(f1, f2);
}

fn brittle(color: vec4<f32>, surface_uv: vec2<f32>, body: f32) -> vec4<f32> {
    let fracture = vec3<f32>(0.00, 0.01, 0.05); // dark gap where the glow breaks apart
    let sheen = vec3<f32>(0.55, 0.75, 1.00);     // cold rim light along the fracture

    let cells = crack_worley(surface_uv * CRACK_SCALE);
    let edge = cells.y - cells.x;
    let line = 1.0 - smoothstep(0.0, CRACK_WIDTH, edge);

    var rgb = mix(color.rgb, sheen, line * 0.5 * body);
    rgb = mix(rgb, fracture, pow(line, 1.2) * CRACK_STRENGTH * body);
    return vec4<f32>(rgb, color.a);
}

@fragment
fn fragment(mesh: VertexOutput) -> @location(0) vec4<f32> {
    let center = vec2<f32>(0.5, 0.5);
    let dist = distance(mesh.uv, center);

    // Center the UV coordinates around (0.0, 0.0)
    let centered_uv: vec2<f32> = mesh.uv * 2.0 - vec2<f32>(1.0, 1.0);

    // Define the small core of the orb
    let core_radius: f32 = 0.3;
    let core_intensity: f32 = smoothstep(core_radius, 0.0, dist);

    // Generate crackling energy bolts using noise
    let angle: f32 = atan2(centered_uv.y, centered_uv.x);
    let radius: f32 = dist;
    let bolt_noise: f32 = noise(vec2<f32>(angle * uniforms.radiance_angle, globals.time * 20.0 - radius * uniforms.radiance_radius));

    // Create bolts by thresholding the noise
    let bolt_intensity: f32 = smoothstep(0.3, 0.5, bolt_noise);

    // Combine core intensity and bolt intensity
    let core_color: vec3<f32> = vec3<f32>(1.0, 1.0, 1.0) * core_intensity;
    let bolt_color: vec3<f32> = vec3<f32>(0.9, 0.9, 0.9) * bolt_intensity;

    // Combine the colors
    let color: vec3<f32> = core_color + bolt_color;

    // Define the maximum radius of the orb
    let max_radius = 0.8;

    // Calculate exponential fade for alpha based on distance
    let edge_fade = exp(-pow((dist / max_radius), 2.0) * 10.0);

    var alpha = 1.;
    // Apply the exponential fade to the alpha channel
    if color.r < 0.6 && color.g < 0.6 && color.b < 0.6 {
        alpha = 0.0;
    } else {
        alpha = clamp(edge_fade, 0.0, 1.0);
    }

    // Output the final color with full opacity
    var final_color = vec4<f32>(color, alpha);
    if (effects.mask & BRITTLE) != 0u {
        final_color = brittle(final_color, mesh.uv, alpha);
    }
    return final_color;
}