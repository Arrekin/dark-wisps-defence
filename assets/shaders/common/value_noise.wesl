// Procedural 2D and 3D value noise computed by hashing integer lattice coordinates.
// Serves as the reference for the texture-backed value noise in `common::noise` and as a
// binding-free fallback for pipelines without the shared GPU group.
//
// `*_drift` functions quantize `velocity` so the periodic lattice repeats an integer number of
// times per `CLOCK_WRAP_PERIOD`.

import package::shaders::common::clock::{CLOCK_WRAP_PERIOD, wrap_rate};
import package::shaders::common::hash::{HASH_GOLDEN, hash_coords, hash_mix, hash_unit};

// Domain salt, so lattice values stay independent of other consumers of the shared mixer.
const VALUE_SALT: u32 = 0x3c6ef372u;

// Wraps integer lattice coordinates into `0..cells - 1` on axes where `cells > 0.0`.
fn periodic_lattice(i: vec3<f32>, cells: vec3<f32>) -> vec3<f32> {
    let folded = i - cells * floor(i / max(cells, vec3<f32>(1.0)));
    return select(i, folded, cells > vec3<f32>(0.0));
}

// Quantizes `velocity` (in lattice cells/s) on each axis so an integer number of cells elapses per
// `CLOCK_WRAP_PERIOD`.
fn fitted_drift(velocity: vec3<f32>) -> vec3<f32> {
    return vec3<f32>(wrap_rate(velocity.x, 1.0), wrap_rate(velocity.y, 1.0), wrap_rate(velocity.z, 1.0));
}

// ── 2D ──────────────────────────────────────────────────────────────────────────────────

fn value_hash_2d(p: vec2<f32>) -> f32 {
    return hash_unit(hash_coords(p, VALUE_SALT));
}

fn value_noise_2d(p: vec2<f32>) -> f32 {
    return value_noise_2d_periodic(p, vec2<f32>(0.0));
}

// 2D value noise tiled every `round(period)` lattice cells on axes where `period > 0.0`.
fn value_noise_2d_periodic(p: vec2<f32>, period: vec2<f32>) -> f32 {
    let cells = round(period);
    let i = floor(p);
    let f = fract(p);
    let near = periodic_lattice(vec3<f32>(i, 0.0), vec3<f32>(cells, 0.0)).xy;
    let far = periodic_lattice(vec3<f32>(i + vec2<f32>(1.0), 0.0), vec3<f32>(cells, 0.0)).xy;
    let a = value_hash_2d(near);
    let b = value_hash_2d(vec2<f32>(far.x, near.y));
    let c = value_hash_2d(vec2<f32>(near.x, far.y));
    let d = value_hash_2d(far);
    let u = f * f * (3.0 - 2.0 * f);
    return mix(mix(a, b, u.x), mix(c, d, u.x), u.y);
}

// 2D value noise advected by `velocity` (lattice cells/s) over wrapped `time`, tiled so the
// pattern repeats seamlessly every `CLOCK_WRAP_PERIOD`. Each drifting axis tiles spatially every
// `|velocity| * CLOCK_WRAP_PERIOD` cells.
fn value_noise_2d_drift(p: vec2<f32>, time: f32, velocity: vec2<f32>) -> f32 {
    let drift = fitted_drift(vec3<f32>(velocity, 0.0)).xy;
    return value_noise_2d_periodic(p + drift * time, abs(drift) * CLOCK_WRAP_PERIOD);
}

fn value_fbm_2d(p: vec2<f32>) -> f32 {
    return value_noise_2d(p) * 0.6
        + value_noise_2d(p * 2.1 + vec2<f32>(4.3, 1.7)) * 0.4;
}

// Two-octave 2D fBm advected by `velocity` over wrapped `time`, seamless across `CLOCK_WRAP_PERIOD`.
fn value_fbm_2d_drift(p: vec2<f32>, time: f32, velocity: vec2<f32>) -> f32 {
    return value_noise_2d_drift(p, time, velocity) * 0.6
        + value_noise_2d_drift(p * 2.1 + vec2<f32>(4.3, 1.7), time, velocity * 2.1) * 0.4;
}

// Combines noise octaves into a value in [0, 1] with mean near 0.5. Requires at least one octave.
// Each octave shifts and scales the sample point to avoid aligning the lattices.
fn value_fbm_2d_octaves(p: vec2<f32>, octaves: i32) -> f32 {
    var total = 0.0;
    var amplitude = 0.5;
    var norm = 0.0;
    var point = p;
    for (var i = 0; i < octaves; i++) {
        total = total + amplitude * value_noise_2d(point);
        norm = norm + amplitude;
        point = point * 2.03 + vec2<f32>(1.7, 9.2);
        amplitude = amplitude * 0.5;
    }
    return total / norm;
}

// Divergence-free curl of a scalar noise potential, producing flow without sources or sinks.
fn value_noise_curl_2d(p: vec2<f32>) -> vec2<f32> {
    let e = 0.5; // Central-difference step in noise-space units.
    return vec2<f32>(
         value_noise_2d(p + vec2<f32>(0.0, e)) - value_noise_2d(p - vec2<f32>(0.0, e)),
        -(value_noise_2d(p + vec2<f32>(e, 0.0)) - value_noise_2d(p - vec2<f32>(e, 0.0)))
    );
}

// Divergence-free 2D curl of `value_noise_2d_drift`.
fn value_noise_curl_2d_drift(p: vec2<f32>, time: f32, velocity: vec2<f32>) -> vec2<f32> {
    let e = 0.5; // Central-difference step in noise-space units.
    return vec2<f32>(
         value_noise_2d_drift(p + vec2<f32>(0.0, e), time, velocity) - value_noise_2d_drift(p - vec2<f32>(0.0, e), time, velocity),
        -(value_noise_2d_drift(p + vec2<f32>(e, 0.0), time, velocity) - value_noise_2d_drift(p - vec2<f32>(e, 0.0), time, velocity))
    );
}

// ── 3D ──────────────────────────────────────────────────────────────────────────────────

// The third axis folds into the 2D hash, so its lattice stays independent of the first two.
fn value_hash_3d(p: vec3<f32>) -> f32 {
    return hash_unit(hash_mix(hash_coords(p.xy, VALUE_SALT) ^ (bitcast<u32>(p.z) * HASH_GOLDEN)));
}

fn value_noise_3d(p: vec3<f32>) -> f32 {
    return value_noise_3d_periodic(p, vec3<f32>(0.0));
}

// 3D value noise tiled every `round(period)` lattice cells on axes where `period > 0.0`.
fn value_noise_3d_periodic(p: vec3<f32>, period: vec3<f32>) -> f32 {
    let cells = round(period);
    let i = floor(p);
    let f = fract(p);
    let u = f * f * (3.0 - 2.0 * f);
    let near = periodic_lattice(i, cells);
    let far = periodic_lattice(i + vec3<f32>(1.0), cells);
    let bottom = mix(
        mix(value_hash_3d(near), value_hash_3d(vec3<f32>(far.x, near.y, near.z)), u.x),
        mix(value_hash_3d(vec3<f32>(near.x, far.y, near.z)), value_hash_3d(vec3<f32>(far.x, far.y, near.z)), u.x),
        u.y,
    );
    let top = mix(
        mix(value_hash_3d(vec3<f32>(near.x, near.y, far.z)), value_hash_3d(vec3<f32>(far.x, near.y, far.z)), u.x),
        mix(value_hash_3d(vec3<f32>(near.x, far.y, far.z)), value_hash_3d(far), u.x),
        u.y,
    );
    return mix(bottom, top, u.z);
}

// 3D value noise advected by `velocity` (lattice cells/s) over wrapped `time`, tiled so the
// pattern repeats seamlessly every `CLOCK_WRAP_PERIOD`. Each drifting axis tiles spatially every
// `|velocity| * CLOCK_WRAP_PERIOD` cells.
fn value_noise_3d_drift(p: vec3<f32>, time: f32, velocity: vec3<f32>) -> f32 {
    let drift = fitted_drift(velocity);
    return value_noise_3d_periodic(p + drift * time, abs(drift) * CLOCK_WRAP_PERIOD);
}

// Normalized by the summed amplitudes, so the output stays in [0, 1] with its mean near 0.5 for any
// octave count. Each octave shifts as well as doubles, so the lattices do not line up.
fn value_fbm_3d(p: vec3<f32>, octaves: i32) -> f32 {
    return value_fbm_3d_periodic(p, octaves, vec3<f32>(0.0));
}

// `value_fbm_3d` tiled every `round(period)` lattice cells on axes where `period > 0.0`.
// Periodic axes use a lacunarity of 2.0 (instead of 2.03) so all octaves share the base period.
fn value_fbm_3d_periodic(p: vec3<f32>, octaves: i32, period: vec3<f32>) -> f32 {
    let periodic = period > vec3<f32>(0.0);
    let scale = select(vec3<f32>(2.03), vec3<f32>(2.0), periodic);
    var total = 0.0;
    var amplitude = 0.5;
    var norm = 0.0;
    var point = p;
    var cells = round(period);
    for (var i = 0; i < octaves; i++) {
        total = total + amplitude * value_noise_3d_periodic(point, cells);
        norm = norm + amplitude;
        point = point * scale + vec3<f32>(1.7, 9.2, 3.1);
        cells = cells * 2.0;
        amplitude = amplitude * 0.5;
    }
    return total / norm;
}

// Multi-octave 3D fBm advected by `velocity` over wrapped `time`, seamless across `CLOCK_WRAP_PERIOD`.
fn value_fbm_3d_drift(p: vec3<f32>, time: f32, velocity: vec3<f32>, octaves: i32) -> f32 {
    let drift = fitted_drift(velocity);
    return value_fbm_3d_periodic(p + drift * time, octaves, abs(drift) * CLOCK_WRAP_PERIOD);
}
