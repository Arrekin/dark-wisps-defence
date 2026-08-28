#define_import_path dwd::gradient_noise
#import dwd::hash::{DWD_HASH_GOLDEN, dwd_hash_coords, dwd_hash_mix, dwd_hash_unit}

// Scales zero-centered interpolated dot products before shifting the output around 0.5.
const DWD_GRADIENT_NOISE_GAIN: f32 = 1.2;

// Domain salt, so lattice gradients stay independent of other consumers of the shared mixer.
const DWD_GRADIENT_SALT: u32 = 0x27d4eb2fu;

// Gradient noise uses a direction per lattice point and is zero at the lattice points themselves,
// so its contours do not follow the grid. Components are uniform over [-1, 1);
// `DWD_GRADIENT_NOISE_GAIN` and callers' mean corrections are calibrated to that distribution.
fn dwd_gradient_at_2d(cell: vec2<f32>) -> vec2<f32> {
    let base = dwd_hash_coords(cell, DWD_GRADIENT_SALT);
    let second = dwd_hash_mix(base ^ DWD_HASH_GOLDEN);
    return vec2<f32>(dwd_hash_unit(base), dwd_hash_unit(second)) * 2.0 - vec2<f32>(1.0);
}

fn dwd_gradient_noise_2d(p: vec2<f32>) -> f32 {
    let base = floor(p);
    let f = p - base;
    // Quintic fade leaves first and second derivatives at zero on lattice lines.
    let u = f * f * f * (f * (f * 6.0 - 15.0) + 10.0);
    let a = dot(dwd_gradient_at_2d(base), f);
    let b = dot(dwd_gradient_at_2d(base + vec2<f32>(1.0, 0.0)), f - vec2<f32>(1.0, 0.0));
    let c = dot(dwd_gradient_at_2d(base + vec2<f32>(0.0, 1.0)), f - vec2<f32>(0.0, 1.0));
    let d = dot(dwd_gradient_at_2d(base + vec2<f32>(1.0, 1.0)), f - vec2<f32>(1.0, 1.0));
    return mix(mix(a, b, u.x), mix(c, d, u.x), u.y) * DWD_GRADIENT_NOISE_GAIN + 0.5;
}

// Rotating as well as doubling each octave prevents aligned lattices from reinforcing their axes.
fn dwd_gradient_fbm_2d(p: vec2<f32>, octaves: i32) -> f32 {
    let rotation = mat2x2<f32>(0.8, -0.6, 0.6, 0.8);
    var total = 0.0;
    var amplitude = 0.5;
    var point = p;
    for (var i = 0; i < octaves; i++) {
        total = total + amplitude * dwd_gradient_noise_2d(point);
        point = rotation * point * 2.0;
        amplitude = amplitude * 0.5;
    }
    return total;
}
