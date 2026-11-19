// Texture-backed value and gradient noise sampled from tileable lattices in the shared GPU group.
//
// - Value noise (`noise_2d`, `noise_3d`): each texel stores one scalar lattice value. Applying a
//   cubic Hermite (smoothstep) curve to the fractional cell coordinate before hardware linear
//   filtering evaluates an octave in a single texture fetch. Tiled at `NOISE_VALUE_2D_CELLS` and
//   `NOISE_VALUE_3D_CELLS` cells per axis.
// - Gradient noise (`noise_gradient_2d`): each texel stores a 2D gradient vector in `RG` (encoded
//   in `[0, 1]`). Each evaluation fetches the four cell corners with two `textureGather` calls and
//   interpolates their dot products with a quintic fade. Tiled at `NOISE_GRADIENT_2D_CELLS` cells
//   per axis.
// - Drifting noise (`*_drift`): advects the sample point at `velocity` (lattice cells/s) on Bevy's
//   shader clock (`time = globals.time`, plus any static offset such as a per-instance seed).
//   Folds completed `CLOCK_WRAP_PERIOD` epochs modulo the texture tile size so advection stays
//   continuous and precise across clock wraps at any velocity.
//
// Requires the shared GPU group (`visuals::shared_gpu`). All texture fetches use explicit MIP
// level 0 (`textureSampleLevel` / `textureGather`) and are valid in non-uniform control flow.

import package::shaders::common::clock::{CLOCK_WRAP_PERIOD, shader_clock_epoch};

// Lattice cells per axis of `noise_value_3d_lattice`. Mirrors the size of `textures/noise/value_3d.ktx2`.
const NOISE_VALUE_3D_CELLS: f32 = 128.0;

// Lattice cells per axis of `noise_value_2d_lattice`. Mirrors the size of `textures/noise/value_2d.ktx2`.
const NOISE_VALUE_2D_CELLS: f32 = 1024.0;

// Lattice cells per axis of `noise_gradient_2d_lattice`. Mirrors the size of `textures/noise/gradient_2d.ktx2`.
const NOISE_GRADIENT_2D_CELLS: f32 = 1024.0;

// Scales zero-centred interpolated dot products before shifting the output around 0.5.
const NOISE_GRADIENT_GAIN: f32 = 1.2;

// fBm octave parameters shared by static and drifting functions.
const NOISE_FBM_LACUNARITY: f32 = 2.03;
const NOISE_FBM_SHIFT: vec3<f32> = vec3<f32>(1.7, 9.2, 3.1);
const NOISE_GRADIENT_FBM_ROTATION: mat2x2<f32> = mat2x2<f32>(0.8, -0.6, 0.6, 0.8);

@group(3) @binding(115) var noise_value_3d_lattice: texture_3d<f32>;
@group(3) @binding(116) var noise_sampler: sampler;
@group(3) @binding(117) var noise_gradient_2d_lattice: texture_2d<f32>;
@group(3) @binding(118) var noise_value_2d_lattice: texture_2d<f32>;

// ── Value noise ─────────────────────────────────────────────────────────────────────────

// 3D value noise at `p` (in lattice cells), returning `[0, 1]`.
fn noise_3d(p: vec3<f32>) -> f32 {
    let cell = floor(p);
    let f = p - cell;
    let fade = f * f * (3.0 - 2.0 * f);
    let folded = cell - NOISE_VALUE_3D_CELLS * floor(cell / NOISE_VALUE_3D_CELLS);
    return textureSampleLevel(noise_value_3d_lattice, noise_sampler, (folded + fade + vec3<f32>(0.5)) / NOISE_VALUE_3D_CELLS, 0.0).x;
}

// Amplitude-normalized 3D value fBm in `[0, 1]` with mean near `0.5` (`octaves >= 1`).
// Translates and scales coordinates between octaves to avoid lattice alignment.
fn noise_fbm_3d(p: vec3<f32>, octaves: i32) -> f32 {
    var total = 0.0;
    var amplitude = 0.5;
    var norm = 0.0;
    var point = p;
    for (var i = 0; i < octaves; i++) {
        total = total + amplitude * noise_3d(point);
        norm = norm + amplitude;
        point = point * NOISE_FBM_LACUNARITY + NOISE_FBM_SHIFT;
        amplitude = amplitude * 0.5;
    }
    return total / norm;
}

// 2D value noise at `p` (in lattice cells), returning `[0, 1]`.
fn noise_2d(p: vec2<f32>) -> f32 {
    let cell = floor(p);
    let f = p - cell;
    let fade = f * f * (3.0 - 2.0 * f);
    let folded = cell - NOISE_VALUE_2D_CELLS * floor(cell / NOISE_VALUE_2D_CELLS);
    return textureSampleLevel(noise_value_2d_lattice, noise_sampler, (folded + fade + vec2<f32>(0.5)) / NOISE_VALUE_2D_CELLS, 0.0).x;
}

// Amplitude-normalized 2D value fBm in `[0, 1]` with mean near `0.5` (`octaves >= 1`).
fn noise_fbm_2d(p: vec2<f32>, octaves: i32) -> f32 {
    var total = 0.0;
    var amplitude = 0.5;
    var norm = 0.0;
    var point = p;
    for (var i = 0; i < octaves; i++) {
        total = total + amplitude * noise_2d(point);
        norm = norm + amplitude;
        point = point * NOISE_FBM_LACUNARITY + NOISE_FBM_SHIFT.xy;
        amplitude = amplitude * 0.5;
    }
    return total / norm;
}

// Divergence-free 2D flow field from the curl of `noise_2d`, using central differences with step `0.5`.
fn noise_curl_2d(p: vec2<f32>) -> vec2<f32> {
    let e = 0.5;
    return vec2<f32>(
         noise_2d(p + vec2<f32>(0.0, e)) - noise_2d(p - vec2<f32>(0.0, e)),
        -(noise_2d(p + vec2<f32>(e, 0.0)) - noise_2d(p - vec2<f32>(e, 0.0))),
    );
}

// ── Gradient noise ──────────────────────────────────────────────────────────────────────

// 2D gradient (Perlin) noise at `p` (in lattice cells), centered on `0.5` (`0.5` at lattice points).
fn noise_gradient_2d(p: vec2<f32>) -> f32 {
    let cell = floor(p);
    let f = p - cell;
    // Quintic Hermite fade with zero first and second derivatives at cell boundaries.
    let u = f * f * f * (f * (f * 6.0 - 15.0) + 10.0);
    let folded = cell - NOISE_GRADIENT_2D_CELLS * floor(cell / NOISE_GRADIENT_2D_CELLS);
    // UV of the shared inner corner of the 2x2 texel quad for `textureGather`.
    let corner = (folded + vec2<f32>(1.0)) / NOISE_GRADIENT_2D_CELLS;
    // `textureGather` component order: x = (0, 1), y = (1, 1), z = (1, 0), w = (0, 0).
    let gradient_x = textureGather(0, noise_gradient_2d_lattice, noise_sampler, corner) * 2.0 - vec4<f32>(1.0);
    let gradient_y = textureGather(1, noise_gradient_2d_lattice, noise_sampler, corner) * 2.0 - vec4<f32>(1.0);
    let offset_x = vec4<f32>(f.x, f.x - 1.0, f.x - 1.0, f.x);
    let offset_y = vec4<f32>(f.y - 1.0, f.y - 1.0, f.y, f.y);
    let dots = gradient_x * offset_x + gradient_y * offset_y;
    return mix(mix(dots.w, dots.z, u.x), mix(dots.x, dots.y, u.x), u.y) * NOISE_GRADIENT_GAIN + 0.5;
}

// Unnormalized 2D gradient fBm with mean `0.5 * (1.0 - pow(0.5, octaves))`. Rotates and doubles
// coordinates each octave to suppress axis-aligned artifacts.
fn noise_gradient_fbm_2d(p: vec2<f32>, octaves: i32) -> f32 {
    var total = 0.0;
    var amplitude = 0.5;
    var point = p;
    for (var i = 0; i < octaves; i++) {
        total = total + amplitude * noise_gradient_2d(point);
        point = NOISE_GRADIENT_FBM_ROTATION * point * 2.0;
        amplitude = amplitude * 0.5;
    }
    return total;
}

// ── Drift ───────────────────────────────────────────────────────────────────────────────

// Computes the sample offset `(velocity * total_time) mod cells` without large-float precision
// loss by folding each completed `CLOCK_WRAP_PERIOD` modulo `cells` before multiplying by
// `shader_clock_epoch`.
fn noise_drift_offset_3d(velocity: vec3<f32>, time: f32, cells: f32) -> vec3<f32> {
    let per_period = velocity * CLOCK_WRAP_PERIOD;
    let folded_per_period = per_period - cells * floor(per_period / cells);
    let completed = folded_per_period * f32(shader_clock_epoch);
    return completed - cells * floor(completed / cells) + velocity * time;
}

fn noise_drift_offset_2d(velocity: vec2<f32>, time: f32, cells: f32) -> vec2<f32> {
    return noise_drift_offset_3d(vec3<f32>(velocity, 0.0), time, cells).xy;
}

// `noise_3d` advected at `velocity` (lattice cells/s).
fn noise_3d_drift(p: vec3<f32>, time: f32, velocity: vec3<f32>) -> f32 {
    return noise_3d(p + noise_drift_offset_3d(velocity, time, NOISE_VALUE_3D_CELLS));
}

// `noise_fbm_3d` advected at `velocity` (lattice cells/s at the base octave, scaled per octave).
fn noise_fbm_3d_drift(p: vec3<f32>, time: f32, velocity: vec3<f32>, octaves: i32) -> f32 {
    var total = 0.0;
    var amplitude = 0.5;
    var norm = 0.0;
    var point = p;
    var octave_velocity = velocity;
    for (var i = 0; i < octaves; i++) {
        total = total + amplitude * noise_3d(point + noise_drift_offset_3d(octave_velocity, time, NOISE_VALUE_3D_CELLS));
        norm = norm + amplitude;
        point = point * NOISE_FBM_LACUNARITY + NOISE_FBM_SHIFT;
        octave_velocity = octave_velocity * NOISE_FBM_LACUNARITY;
        amplitude = amplitude * 0.5;
    }
    return total / norm;
}

// `noise_2d` advected at `velocity` (lattice cells/s).
fn noise_2d_drift(p: vec2<f32>, time: f32, velocity: vec2<f32>) -> f32 {
    return noise_2d(p + noise_drift_offset_2d(velocity, time, NOISE_VALUE_2D_CELLS));
}

// `noise_fbm_2d` advected at `velocity` (lattice cells/s at the base octave, scaled per octave).
fn noise_fbm_2d_drift(p: vec2<f32>, time: f32, velocity: vec2<f32>, octaves: i32) -> f32 {
    var total = 0.0;
    var amplitude = 0.5;
    var norm = 0.0;
    var point = p;
    var octave_velocity = velocity;
    for (var i = 0; i < octaves; i++) {
        total = total + amplitude * noise_2d(point + noise_drift_offset_2d(octave_velocity, time, NOISE_VALUE_2D_CELLS));
        norm = norm + amplitude;
        point = point * NOISE_FBM_LACUNARITY + NOISE_FBM_SHIFT.xy;
        octave_velocity = octave_velocity * NOISE_FBM_LACUNARITY;
        amplitude = amplitude * 0.5;
    }
    return total / norm;
}

// `noise_curl_2d` with its scalar potential advected at `velocity` (lattice cells/s).
fn noise_curl_2d_drift(p: vec2<f32>, time: f32, velocity: vec2<f32>) -> vec2<f32> {
    return noise_curl_2d(p + noise_drift_offset_2d(velocity, time, NOISE_VALUE_2D_CELLS));
}

// `noise_gradient_2d` advected at `velocity` (lattice cells/s).
fn noise_gradient_2d_drift(p: vec2<f32>, time: f32, velocity: vec2<f32>) -> f32 {
    return noise_gradient_2d(p + noise_drift_offset_2d(velocity, time, NOISE_GRADIENT_2D_CELLS));
}

// `noise_gradient_fbm_2d` advected at `velocity` (lattice cells/s at the base octave, rotated and
// scaled per octave).
fn noise_gradient_fbm_2d_drift(p: vec2<f32>, time: f32, velocity: vec2<f32>, octaves: i32) -> f32 {
    var total = 0.0;
    var amplitude = 0.5;
    var point = p;
    var octave_velocity = velocity;
    for (var i = 0; i < octaves; i++) {
        total = total + amplitude * noise_gradient_2d(point + noise_drift_offset_2d(octave_velocity, time, NOISE_GRADIENT_2D_CELLS));
        point = NOISE_GRADIENT_FBM_ROTATION * point * 2.0;
        octave_velocity = NOISE_GRADIENT_FBM_ROTATION * octave_velocity * 2.0;
        amplitude = amplitude * 0.5;
    }
    return total;
}
