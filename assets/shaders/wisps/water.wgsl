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

const circle_radius: f32 = 0.64;
const edge_softness: f32 = 0.35;

// ── Procedural brittle cracks (milestone 1) ─────────────────────────────────
// A cellular/Worley crack web, sampled in the wisp's distorted surface space so
// the fractures ride the same bounce as the water body. Tunables up top.
const CRACK_SCALE: f32 = 5.5;     // cells across the wisp; higher = finer web
const CRACK_WIDTH: f32 = 0.07;    // fracture line thickness
const CRACK_STRENGTH: f32 = 0.95; // overall opacity of the effect

fn crack_hash2(p: vec2<f32>) -> vec2<f32> {
    let k = vec2<f32>(
        dot(p, vec2<f32>(127.1, 311.7)),
        dot(p, vec2<f32>(269.5, 183.3)),
    );
    return fract(sin(k) * 43758.5453123);
}

// Returns (F1, F2): distance to the nearest and second-nearest cell points.
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

// Brittle look for the WATER wisp. `surface_uv` is the distorted UV (rides the
// bounce); `body` is the wisp's coverage so cracks stay inside it.
fn brittle(color: vec4<f32>, surface_uv: vec2<f32>, body: f32) -> vec4<f32> {
    let glow = vec3<f32>(0.80, 0.95, 1.00); // icy highlight along the fracture
    let core = vec3<f32>(0.00, 0.08, 0.22); // dark hairline at its centre

    let cells = crack_worley(surface_uv * CRACK_SCALE);
    let edge = cells.y - cells.x;                  // ~0 along cell borders
    let line = 1.0 - smoothstep(0.0, CRACK_WIDTH, edge);
    let amount = line * CRACK_STRENGTH * body;

    var rgb = mix(color.rgb, glow, amount);
    rgb = mix(rgb, core, pow(line, 4.0) * CRACK_STRENGTH * body);
    return vec4<f32>(rgb, color.a);
}

@fragment
fn fragment(mesh: VertexOutput) -> @location(0) vec4<f32> {
    let center = vec2<f32>(0.5, 0.5);
    let dist = distance(mesh.uv, center);
    let amplitude = uniforms.amplitude;  // 0.06 - 0.14
    let frequency =  uniforms.frequency; // 2. - 2.6
    let speed = uniforms.speed; // 3.0 - 6.0

    //let amplitude = 0.24311;
    //let frequency =  2.;   // This gives worm like movement effect

    // Calculate distortion based on UV coordinates and time
    let offset = vec2<f32>(
        amplitude * sin(mesh.uv.y * frequency + globals.time * speed * uniforms.sinus_direction),
        amplitude * cos(mesh.uv.x * frequency + globals.time * speed * uniforms.cosinus_direction) 
    );


    // Apply distortion to UV coordinates
    let distorted_uv = mesh.uv + offset;

    // Sample the texture with distorted UV
    var color = vec4<f32>(0., 0., 1.0 - dist * 2., 1.0 - dist * 2.);
    color = textureSample(wisp_tex1, wisp_tex1_sampler, distorted_uv) + textureSample(wisp_tex2, wisp_tex1_sampler, distorted_uv);
    color.b = 1.;
    color.g += 0.07;
    color.r += 0.07;
    let mask = smoothstep(circle_radius, circle_radius - edge_softness, dist);
    color.a *= mask;
    // Get rid of non-black parts
    if color.g > 0.2 {
        color.a = 0.0;
    }
    
    // Hardcoded brittle for visual testing (milestone 1) — every water wisp cracks.
    color = brittle(color, distorted_uv, color.a);

    return color;
}