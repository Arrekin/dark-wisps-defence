import bevy_sprite_render::mesh2d::vertex_output::VertexOutput;
import package::shaders::common::fire_ramp::fire_ramp;
import package::shaders::common::hash::{hash_coords, hash_mix, hash_unit};
import package::shaders::common::value_noise::{value_fbm_3d, value_noise_3d};

// `InfernoDomeFire` shader: full-radius hemisphere that erodes in place via rising noise thresholds.
//
// Layers (composited in world units relative to dome center, premultiplied alpha):
// - `skin`: outer hemispherical shell with limb thickening and a finer, slower-eroding rim lattice.
// - `inner_fire`: raymarched interior volume (`INNER_STEPS`) that outlasts `skin` by `INNER_LAG`.
// - `spill`: additive ground glow outside the rim during breakup.
// - `embers`: additive falling particles spawned on a 2D cell grid (`EMBER_CELL`).

// Field order and types mirror `InfernoDomeUniform` in weaponry_internal/src/inferno_dome.rs.
struct InfernoDome {
    age: f32,        // elapsed seconds since spawn
    radius: f32,     // dome radius in world units
    seed: f32,       // per-entity noise offset
    half_size: f32,  // quad half-size in world units
};

@group(2) @binding(0) var<uniform> dome: InfernoDome;

// Heat
const HEAT_CAP: f32 = 0.97;
const HOT_RIM: f32 = 0.25;
const HOT_RIM_FROM: f32 = 0.45;    // normalized radius where rim heat boost begins

// Unit directional light vector for hemisphere shading
const DOME_LIGHT: vec3<f32> = vec3<f32>(-0.48, 0.64, 0.60);

// Skin
const SKIN_HEAT: f32 = 0.6;
const SHELL: f32 = 0.12;            // shell thickness as a fraction of `radius`
const INTERIOR: f32 = 0.05;         // baseline density inside the inner shell radius
const FLAME_CELL: f32 = 16.0;
const CHURN: f32 = 1.3;
const RAGGED_RIM: f32 = 5.0;
const OPACITY: f32 = 0.9;
const HARD_EDGES: f32 = 0.8;        // blend factor toward step-like alpha (0 = linear, 1 = smoothstepped)
const SHADING: f32 = 0.6;
const HIGHLIGHT: f32 = 0.6;
const APEX_HEAT: f32 = 0.4;
const PUNCH: f32 = 0.45;            // initial flash heat boost

// Breakup
const BREAKUP_TIME: f32 = 0.9;
const HOLE_SOFTNESS: f32 = 0.16;
const HOLE_GLOW: f32 = 0.5;
const THIN_OUT: f32 = 0.5;
const COOLING: f32 = 0.6;

// Rim ember lattice (finer, slower-eroding noise blended in toward the outer edge)
const LATTICE_FROM: f32 = 0.2;      // normalized radius where lattice blend begins
const LATTICE_CELL: f32 = 0.62;     // noise scale relative to `FLAME_CELL`
const LATTICE_SLOWDOWN: f32 = 1.45; // breakup duration multiplier at the rim

// Inner fire
const INNER_STEPS: i32 = 8;
const INNER_DENSITY: f32 = 0.6;
const INNER_CELL: f32 = 18.0;
const INNER_SPARSENESS: f32 = 0.53;
const INNER_HEAT: f32 = 0.58;
const INNER_CORE_HEAT: f32 = 0.28;
const INNER_RISE: f32 = 1.0;
const INNER_LAG: f32 = 0.4;         // extra lifetime fraction relative to `BREAKUP_TIME`

// Rim spill light
const SPILL: f32 = 0.8;
const SPILL_WIDTH: f32 = 14.0;

// Falling embers (at most 1 particle per `EMBER_CELL`; max drift < 1 cell, so 3x3 neighborhood suffices)
const EMBER_CELL: f32 = 14.0;
const EMBER_CENTER_CHANCE: f32 = 0.12;
const EMBER_RIM_FROM: f32 = 0.45;
const EMBER_FALL_TIME: f32 = 0.55;
const EMBER_DELAY: f32 = 0.8;            // max spawn delay as a fraction of `BREAKUP_TIME`
const EMBER_FALL_SHORTEST: f32 = 0.6;    // min fall duration multiplier on `EMBER_FALL_TIME`
const EMBER_FALL_SPREAD: f32 = 0.8;      // fall duration range multiplier on `EMBER_FALL_TIME`
const EMBER_SALT: u32 = 0x5be1u;

fn hard_alpha(alpha: f32) -> f32 {
    return mix(alpha, smoothstep(0.2, 0.5, alpha), HARD_EDGES);
}

fn flame_color(heat: f32) -> vec3<f32> {
    return fire_ramp(clamp(heat, 0.0, HEAT_CAP));
}

fn rim_heat(r: f32, n: f32, veins: f32) -> f32 {
    return HOT_RIM * smoothstep(HOT_RIM_FROM, 1.0, r) * smoothstep(0.35, 0.75, n) * (0.6 + 0.8 * veins);
}

// Outer hemisphere shell (premultiplied). `progress = age / BREAKUP_TIME`.
fn skin(p: vec2<f32>, radius: f32, age: f32, progress: f32) -> vec4<f32> {
    let d = length(p);
    if d > radius + RAGGED_RIM || progress >= LATTICE_SLOWDOWN {
        return vec4<f32>(0.0);
    }
    let r = d / radius;
    let churn = CHURN * age + dome.seed;
    // Sample rim noise via the unit direction vector to avoid an angular seam at +-pi.
    let around = p / max(d, 0.001) * radius / 18.0;
    let rim_noise = value_fbm_3d(vec3<f32>(around, churn * 1.7), 3);
    let inside = smoothstep(1.0, 0.985, r + (rim_noise - 0.5) * 2.0 * RAGGED_RIM / radius);
    if inside <= 0.0 {
        return vec4<f32>(0.0);
    }

    // Hemisphere surface point and normal; `path` is vertical chord length through the outer shell.
    let rc = min(r, 0.999);
    let z = sqrt(1.0 - rc * rc);
    let normal = vec3<f32>(p / radius, z);
    let inner = 1.0 - SHELL;
    let path = (z - sqrt(max(inner * inner - rc * rc, 0.0))) / sqrt(1.0 - inner * inner);
    let shell = mix(INTERIOR, 1.0, clamp(path, 0.0, 1.0));

    // Domain-warped 3D noise evaluated on the hemisphere normal.
    let s = normal * radius / FLAME_CELL + vec3<f32>(dome.seed * 3.7);
    let warp = vec3<f32>(
        value_fbm_3d(s * 0.6 + vec3<f32>(3.1, 1.7, churn), 3),
        value_fbm_3d(s * 0.6 + vec3<f32>(-5.2, 8.3, churn + 2.0), 3),
        value_fbm_3d(s * 0.6 + vec3<f32>(6.4, -2.9, churn + 5.0), 3),
    ) - vec3<f32>(0.5);
    let sw = s + warp * 1.4;
    let coarse = value_fbm_3d(sw, 5);
    let coarse_veins = 1.0 - abs(value_fbm_3d(sw * 2.1 + vec3<f32>(7.0, -3.0, 0.0), 3) * 2.0 - 1.0);
    let fine_point = sw / LATTICE_CELL + vec3<f32>(11.0, -4.0, 0.0);
    let fine = value_fbm_3d(fine_point, 5);
    let fine_veins = 1.0 - abs(value_fbm_3d(fine_point * 2.1 + vec3<f32>(7.0, -3.0, 0.0), 3) * 2.0 - 1.0);
    let lattice = smoothstep(LATTICE_FROM, 1.0, r);
    let n = mix(coarse, fine, lattice);
    let veins = mix(coarse_veins, fine_veins, lattice);

    // Rising threshold erodes holes across the shell.
    let b = clamp(progress / mix(1.0, LATTICE_SLOWDOWN, lattice), 0.0, 1.0);
    let threshold = mix(-0.35, 1.05, pow(b, 0.75));
    let field = n * 0.75 + 0.25 * veins;
    let mask = smoothstep(threshold, threshold + mix(HOLE_SOFTNESS, 0.05, lattice), field);
    let hole_edge = mask * (1.0 - mask) * 4.0;
    let density = clamp(shell * (0.55 + 0.6 * n), 0.0, 1.0);
    let alpha = hard_alpha(inside * mask * density * OPACITY * (1.0 - mix(THIN_OUT, 0.15, lattice) * b));

    // Diffuse + specular hemisphere lighting.
    let lambert = max(dot(normal, DOME_LIGHT), 0.0);
    let shade = mix(1.0, 0.62 + 0.55 * lambert, SHADING);
    let spec = pow(max(dot(normal, normalize(DOME_LIGHT + vec3<f32>(0.0, 0.0, 1.0))), 0.0), 10.0) * HIGHLIGHT;
    let heat = SKIN_HEAT * (1.0 - COOLING * 0.55 * b) + 0.3 * (n - 0.5) + 0.12 * (veins - 0.5)
        + APEX_HEAT * 0.22 * (z - 0.55) + 0.14 * spec + PUNCH * 0.25 * exp(-age * 14.0)
        + mix(HOLE_GLOW, 1.0, lattice) * 0.18 * hole_edge + rim_heat(r, n, veins);
    let color = flame_color(heat) * alpha * 1.25 * shade + flame_color(heat + 0.1) * spec * alpha * 0.45;
    return vec4<f32>(color, alpha);
}

// Front-to-back vertical raymarch through the dome interior (premultiplied).
fn inner_fire(p: vec2<f32>, radius: f32, age: f32, b: f32) -> vec4<f32> {
    let r = length(p) / radius;
    if r >= 1.0 || b >= 1.0 {
        return vec4<f32>(0.0);
    }
    let top = sqrt(1.0 - r * r) * radius;
    let step_length = top / f32(INNER_STEPS);
    let threshold = INNER_SPARSENESS + (1.05 - INNER_SPARSENESS) * pow(b, 1.6);
    var color = vec3<f32>(0.0);
    var transmit = 1.0;
    for (var i = 0; i < INNER_STEPS; i++) {
        let point = vec3<f32>(p, top - (f32(i) + 0.5) * step_length);
        let q = point / INNER_CELL + vec3<f32>(dome.seed * 1.3, dome.seed * 0.7, -INNER_RISE * age * 2.0);
        let warp = vec3<f32>(
            value_fbm_3d(q * 0.5 + vec3<f32>(2.0, 0.0, age), 2),
            value_fbm_3d(q * 0.5 + vec3<f32>(0.0, 5.0, age), 2),
            0.5,
        ) - vec3<f32>(0.5);
        let n = value_fbm_3d(q + warp * 1.6, 4);
        let density = smoothstep(threshold, threshold + 0.12, n) * INNER_DENSITY;
        let coreness = 1.0 - length(point) / radius;
        let heat = INNER_HEAT * (1.0 - COOLING * 0.5 * b) + INNER_CORE_HEAT * coreness + 0.35 * (n - threshold)
            + rim_heat(length(point) / radius, n, 0.5);
        let alpha = 1.0 - exp(-density * step_length / 14.0);
        color += transmit * flame_color(heat) * alpha * 1.2;
        transmit *= 1.0 - alpha;
    }
    let edge = smoothstep(1.0, 0.95, r);
    let soft = (1.0 - transmit) * edge * OPACITY;
    let hard = hard_alpha(soft);
    return vec4<f32>(color * edge * hard / max(soft, 0.0001), hard);
}

// Additive falling embers evaluated across the 3x3 neighborhood of `EMBER_CELL` cells.
fn embers(p: vec2<f32>, radius: f32, age: f32) -> vec3<f32> {
    if length(p) > radius + EMBER_CELL || age >= EMBER_DELAY * BREAKUP_TIME + (EMBER_FALL_SHORTEST + EMBER_FALL_SPREAD) * EMBER_FALL_TIME {
        return vec3<f32>(0.0);
    }
    let home = floor(p / EMBER_CELL);
    var color = vec3<f32>(0.0);
    for (var dy = -1; dy <= 1; dy++) {
        for (var dx = -1; dx <= 1; dx++) {
            let cell = home + vec2<f32>(f32(dx), f32(dy));
            var h = hash_coords(cell + vec2<f32>(dome.seed), EMBER_SALT);
            let jitter = vec2<f32>(hash_unit(h), hash_unit(hash_mix(h + 1u)));
            let start = (cell + jitter) * EMBER_CELL;
            let start_r = length(start) / radius;
            h = hash_mix(h + 2u);
            let chance = mix(EMBER_CENTER_CHANCE, 1.0, smoothstep(EMBER_RIM_FROM, 0.92, start_r));
            if start_r > 1.0 || hash_unit(h) > chance {
                continue;
            }
            let delay = hash_unit(hash_mix(h + 3u)) * EMBER_DELAY * BREAKUP_TIME;
            let fall_time = EMBER_FALL_TIME * (EMBER_FALL_SHORTEST + EMBER_FALL_SPREAD * hash_unit(hash_mix(h + 4u)));
            let t = age - delay;
            let f = t / fall_time;
            if f < 0.0 || f >= 1.0 {
                continue;
            }
            let drift_x = (hash_unit(hash_mix(h + 5u)) - 0.5) * 10.0;
            let drift_y = -4.0 - 1.5 * hash_unit(hash_mix(h + 6u));
            let position = start + vec2<f32>(
                drift_x * (1.0 - exp(-2.0 * t)) * 0.5 + 0.6 * sin(t * 7.0 + jitter.x * 40.0),
                drift_y * t,
            );
            let size = (1.4 + 2.4 * hash_unit(hash_mix(h + 7u))) * (1.0 - 0.65 * f) + 0.8;
            let k = smoothstep(size * 0.5, 0.0, length(p - position));
            let heat = 0.55 + 0.3 * hash_unit(hash_mix(h + 8u)) - 0.35 * f;
            color += flame_color(heat) * k * k * 1.3 * (1.0 - f * f);
        }
    }
    return color;
}

@fragment
fn fragment(mesh: VertexOutput) -> @location(0) vec4<f32> {
    let p = (vec2<f32>(mesh.uv.x, 1.0 - mesh.uv.y) - vec2<f32>(0.5)) * 2.0 * dome.half_size;
    let age = dome.age;
    let radius = dome.radius;
    let progress = age / BREAKUP_TIME;

    var result = inner_fire(p, radius, age, clamp(age / (BREAKUP_TIME * (1.0 + INNER_LAG)), 0.0, 1.0));
    let outer = skin(p, radius, age, progress);
    result = outer + result * (1.0 - outer.a);

    // Additive ground glow outside the rim during breakup (`past_rim < 3.0` bounds Gaussian falloff).
    let r = length(p) / radius;
    let past_rim = (r - 1.0) * radius / SPILL_WIDTH;
    var spill = 0.0;
    if progress < 1.0 && past_rim < 3.0 {
        spill = SPILL * smoothstep(0.97, 1.02, r) * exp(-past_rim * past_rim) * (1.0 - progress)
            * (0.7 + 0.6 * value_noise_3d(vec3<f32>(p / 12.0, age * 2.0)));
    }
    let added = flame_color(0.5) * spill * 0.45 + embers(p, radius, age);
    return vec4<f32>(result.rgb + added, result.a);
}
