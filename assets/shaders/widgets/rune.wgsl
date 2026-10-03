#import bevy_ui::ui_vertex_output::{UiVertexOutput}
#import bevy_render::globals::{Globals}

// A glyph with a fixed spine and twelve seed-selected branches.
//
//      TL     CT     TR          spine: CT-CM-CB, always drawn
//        \    |    /
//      ML --- CM --- MR
//        /    |    \
//      BL     CB     BR

struct RuneParams {
    // Twelve bits, one per optional branch.
    seed: u32,
    stroke_width: f32,
    // Radians. Small per-glyph rotations keep a column of runes from looking stamped.
    tilt: f32,
    // Seconds on the same clock as globals.time.
    start_time: f32,
};

// The glyph's fade is a function of its age alone, so the whole curve lives here and the
// material is written once, when the rune is spawned.
struct RuneLife {
    duration: f32,
    // Fractions of the flight spent fading in and out.
    fade_in: f32,
    fade_out: f32,
    brightness: f32,
};

struct RuneMaterial {
    color: vec4<f32>,
    params: RuneParams,
    life: RuneLife,
};

@group(0) @binding(1)
var<uniform> globals: Globals;

@group(1) @binding(0)
var<uniform> material: RuneMaterial;

// Node positions in the glyph's unit box, inset so strokes are not clipped by the quad.
const CT: vec2<f32> = vec2<f32>(0.50, 0.10);
const CM: vec2<f32> = vec2<f32>(0.50, 0.50);
const CB: vec2<f32> = vec2<f32>(0.50, 0.90);
const TL: vec2<f32> = vec2<f32>(0.14, 0.10);
const TR: vec2<f32> = vec2<f32>(0.86, 0.10);
const ML: vec2<f32> = vec2<f32>(0.14, 0.50);
const MR: vec2<f32> = vec2<f32>(0.86, 0.50);
const BL: vec2<f32> = vec2<f32>(0.14, 0.90);
const BR: vec2<f32> = vec2<f32>(0.86, 0.90);

// Distance from `p` to the segment `a`-`b`.
fn stroke(p: vec2<f32>, a: vec2<f32>, b: vec2<f32>) -> f32 {
    let pa = p - a;
    let ba = b - a;
    let h = clamp(dot(pa, ba) / dot(ba, ba), 0.0, 1.0);
    return length(pa - ba * h);
}

fn has_branch(bits: u32, index: u32) -> bool {
    return ((bits >> index) & 1u) == 1u;
}

@fragment
fn fragment(in: UiVertexOutput) -> @location(0) vec4<f32> {
    let tilt = material.params.tilt;
    let rotation = mat2x2<f32>(cos(tilt), -sin(tilt), sin(tilt), cos(tilt));
    let p = rotation * (in.uv - vec2<f32>(0.5)) + vec2<f32>(0.5);

    let bits = material.params.seed;

    // Spine. Present in every glyph, which is most of why they look related.
    var d = stroke(p, CT, CM);
    d = min(d, stroke(p, CM, CB));

    if (has_branch(bits, 0u))  { d = min(d, stroke(p, TL, CT)); }
    if (has_branch(bits, 1u))  { d = min(d, stroke(p, TR, CT)); }
    if (has_branch(bits, 2u))  { d = min(d, stroke(p, TL, CM)); }
    if (has_branch(bits, 3u))  { d = min(d, stroke(p, TR, CM)); }
    if (has_branch(bits, 4u))  { d = min(d, stroke(p, ML, CM)); }
    if (has_branch(bits, 5u))  { d = min(d, stroke(p, MR, CM)); }
    if (has_branch(bits, 6u))  { d = min(d, stroke(p, BL, CM)); }
    if (has_branch(bits, 7u))  { d = min(d, stroke(p, BR, CM)); }
    if (has_branch(bits, 8u))  { d = min(d, stroke(p, BL, CB)); }
    if (has_branch(bits, 9u))  { d = min(d, stroke(p, BR, CB)); }
    if (has_branch(bits, 10u)) { d = min(d, stroke(p, TL, BL)); }
    if (has_branch(bits, 11u)) { d = min(d, stroke(p, TR, BR)); }

    let age = globals.time - material.params.start_time;
    let progress = clamp(age / max(material.life.duration, 1e-4), 0.0, 1.0);
    let rising = clamp(progress / max(material.life.fade_in, 1e-4), 0.0, 1.0);
    let falling = clamp((1.0 - progress) / max(material.life.fade_out, 1e-4), 0.0, 1.0);
    let brightness = min(rising, falling) * material.life.brightness;

    let half_width = material.params.stroke_width * 0.5;
    let aa = fwidth(d);
    let ink = 1.0 - smoothstep(half_width - aa, half_width + aa, d);
    let alpha = ink * brightness * material.color.a;
    if (alpha < 0.002) {
        return vec4<f32>(0.0);
    }
    return vec4<f32>(material.color.rgb, alpha);
}
