#import bevy_sprite::mesh2d_vertex_output::VertexOutput
#import bevy_sprite::mesh2d_view_bindings::globals

struct UniformData {
    amplitude: f32,
    frequency: f32,
    speed: f32, // animation speed
    sinus_direction: f32, // -1 or 1
    cosinus_direction: f32, // -1 or 1
};

@group(2) @binding(4)
var<uniform> uniforms: UniformData;

@group(2) @binding(0) var wisp_tex1: texture_2d<f32>;
@group(2) @binding(1) var wisp_tex1_sampler: sampler;
@group(2) @binding(2) var wisp_tex2: texture_2d<f32>;

const WISP_EFFECT_SLOTS: u32 = 8u;
struct WispEffects {
    mask: u32,
    params: array<vec4<f32>, WISP_EFFECT_SLOTS>,
};
@group(2) @binding(5)
var<uniform> effects: WispEffects;

const BRITTLE: u32 = 1u;

const circle_radius: f32 = 0.64;
const edge_softness: f32 = 0.35;

// ── Procedural brittle crack ────────────────────────────────────────────────
// A bold three-armed fracture splitting the core from its centre. The core is a small disc,
// so a single deterministic split reads far better than a fine web that vanishes at this
// size. Fire fractures into a charred crust with hot ember light escaping the seam.
const TAU: f32 = 6.28318530718;
const CRACK_ARMS: f32 = 3.0;
const CRACK_ARM_WIDTH: f32 = 0.04; // arm half-thickness, in UV
const CRACK_ARM_OFFSET: f32 = 0.7; // rotation of the star, in radians

fn brittle(color: vec4<f32>, uv: vec2<f32>, body: f32) -> vec4<f32> {
    let char_crust = vec3<f32>(0.03, 0.01, 0.00); // cooled crust along the fracture
    let ember = vec3<f32>(1.00, 0.50, 0.10);       // hot light escaping the seam

    let p = uv - vec2<f32>(0.5);
    let r = length(p);
    let a = atan2(p.y, p.x);
    let t = (a - CRACK_ARM_OFFSET) * CRACK_ARMS / TAU;
    let frac = t - floor(t + 0.5);                  // 0 on an arm, +/-0.5 between arms
    let perp = r * abs(frac) * TAU / CRACK_ARMS;    // distance to the nearest arm
    let line = 1.0 - smoothstep(CRACK_ARM_WIDTH * 0.5, CRACK_ARM_WIDTH, perp);

    var rgb = mix(color.rgb, char_crust, line * body);
    rgb = mix(rgb, ember, pow(line, 2.0) * body);
    return vec4<f32>(rgb, color.a);
}

@fragment
fn fragment(mesh: VertexOutput) -> @location(0) vec4<f32> {
    let center = vec2<f32>(0.5, 0.5);
    let dist = distance(mesh.uv, center);
    let amplitude = uniforms.amplitude;  // 0.2 - 0.45, 0.4311
    let frequency =  uniforms.frequency; // 15 - 22, 15.5
    let speed = uniforms.speed; // 3-7, 5.

    // Calculate distortion based on UV coordinates and time
    let offset = vec2<f32>(
        amplitude * sin(mesh.uv.y * frequency + globals.time * speed * uniforms.sinus_direction), 
        amplitude * cos(mesh.uv.x * frequency + globals.time * speed * uniforms.cosinus_direction) 
    );


    // Apply distortion to UV coordinates
    let distorted_uv = mesh.uv + offset;

    // Sample the texture with distorted UV
    var color = vec4<f32>(1.0 - dist * 2., 0., 0., 1.0 - dist * 2.);
    if dist > 0.25 {
        color = textureSample(wisp_tex1, wisp_tex1_sampler, distorted_uv) + textureSample(wisp_tex2, wisp_tex1_sampler, distorted_uv);
        color.r = 1.;
        color.g += 0.07;
        color.b += 0.07;
        let mask = smoothstep(circle_radius, circle_radius - edge_softness, dist);
        color.a *= mask;
        // Get rid of non-black parts
        if color.g > 0.2 {
            color.a = 0.0;
        }
    } else if dist > 0.20 {
        // Make a gap between the wisp core and the aura effect
        color.a = 0.;
    }
    
    if (effects.mask & BRITTLE) != 0u {
        // Split only the solid core, leaving the surrounding flame intact.
        let core_mask = color.a * (1.0 - smoothstep(0.16, 0.22, dist));
        color = brittle(color, mesh.uv, core_mask);
    }

    return color;
}