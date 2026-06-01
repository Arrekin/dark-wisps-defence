#import bevy_sprite::mesh2d_vertex_output::VertexOutput
#import bevy_sprite::mesh2d_view_bindings::globals

struct UniformData {
    angle_direction: f32,  // -1 or 1
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

// Fractal Brownian Motion function to create complex patterns
fn fbm(p: vec2<f32>) -> f32 {
    var pp = p;
    var value: f32 = 0.0;
    var amplitude: f32 = 0.5;
    var frequency: f32 = 0.0;
    for (var i: i32 = 0; i < 5; i = i + 1) {
        value += amplitude * noise(pp);
        pp = pp * 2.0;
        amplitude *= 0.5;
    }
    return value;
}

// ── Procedural brittle crack ────────────────────────────────────────────────
// A bold three-armed fracture splitting the orb core from its centre — a dark crackle with a
// bright charged seam that flickers with the current. The core is small, so a single
// deterministic split reads far better than a fine web.
const TAU: f32 = 6.28318530718;
const CRACK_ARMS: f32 = 3.0;
const CRACK_ARM_WIDTH: f32 = 0.04; // arm half-thickness, in UV
const CRACK_ARM_OFFSET: f32 = 1.9; // rotation of the star, in radians

fn brittle(color: vec4<f32>, uv: vec2<f32>, body: f32) -> vec4<f32> {
    let spark = vec3<f32>(1.00, 1.00, 0.60); // bright charged seam
    let dead = vec3<f32>(0.00, 0.00, 0.00);  // dark crackle

    let p = uv - vec2<f32>(0.5);
    let r = length(p);
    let a = atan2(p.y, p.x);
    let t = (a - CRACK_ARM_OFFSET) * CRACK_ARMS / TAU;
    let frac = t - floor(t + 0.5);
    let perp = r * abs(frac) * TAU / CRACK_ARMS;
    let line = 1.0 - smoothstep(CRACK_ARM_WIDTH * 0.5, CRACK_ARM_WIDTH, perp);
    let flicker = 0.6 + 0.4 * sin(globals.time * 30.0 + a * 3.0);

    var rgb = mix(color.rgb, dead, line * body);
    rgb = mix(rgb, spark, pow(line, 2.0) * body * flicker);
    return vec4<f32>(rgb, color.a);
}

@fragment
fn fragment(mesh: VertexOutput) -> @location(0) vec4<f32> {
    let center = vec2(0.5, 0.5);
    let dist = distance(mesh.uv, center);

    // Center the UV coordinates around (0.0, 0.0)
    let centered_uv = mesh.uv * 2.0 - vec2(1.0, 1.0);

    // Define the small core of the orb
    let core_radius = 0.2;
    let core_intensity = smoothstep(core_radius, 0.0, dist);

    // Generate lightning-like bolts using Fractal Brownian Motion
    let angle = atan2(centered_uv.y, centered_uv.x);
    let radius = dist;

    // Adjust time to control the speed
    let time_factor = globals.time * 5.5;

    // Create a complex pattern for the bolts with reduced frequency
    let bolt_pattern = fbm(vec2(
        angle * 5.0 + time_factor * 2.0 * uniforms.angle_direction,
        radius * 3.0 - time_factor * 2.0
    ));

    // Create bolts by thresholding the pattern
    let bolt_intensity = smoothstep(0.45, 0.75, bolt_pattern);

    // Enhance the bolt appearance
    let bolt_shape = pow(bolt_intensity, 2.0);

    // Combine core intensity and bolt intensity
    let core_color = vec3(1.0, 1.0, 0.0) * core_intensity;
    let bolt_color = vec3(1.0, 1.0, 0.2) * bolt_shape;

    // Combine the colors
    let color = core_color + bolt_color;

    // Define the maximum radius of the orb
    let max_radius = 1.;

    // Calculate exponential fade for alpha based on distance
    let edge_fade = exp(-pow((dist / max_radius), 2.0) * 10.0);

    var alpha = 1.0;
    // Apply the exponential fade to the alpha channel
    if color.r < 0.1 && color.g < 0.1 && color.b < 0.1 {
        alpha = 0.0;
    } else {
        alpha = clamp(edge_fade, 0.0, 1.0);
    }

    // Output the final color with full opacity
    var final_color = vec4(color, alpha);
    if (effects.mask & BRITTLE) != 0u {
        // Split only the orb core, leaving the bolts intact.
        final_color = brittle(final_color, mesh.uv, core_intensity);
    }
    return final_color;
}