#define_import_path dwd::value_noise
#import dwd::hash::{hash_coords, hash_unit}

// Domain salt, so lattice values stay independent of other consumers of the shared mixer.
const VALUE_SALT: u32 = 0x3c6ef372u;

fn value_hash_2d(p: vec2<f32>) -> f32 {
    return hash_unit(hash_coords(p, VALUE_SALT));
}

fn value_noise_2d(p: vec2<f32>) -> f32 {
    let i = floor(p);
    let f = fract(p);
    let a = value_hash_2d(i);
    let b = value_hash_2d(i + vec2<f32>(1.0, 0.0));
    let c = value_hash_2d(i + vec2<f32>(0.0, 1.0));
    let d = value_hash_2d(i + vec2<f32>(1.0, 1.0));
    let u = f * f * (3.0 - 2.0 * f);
    return mix(mix(a, b, u.x), mix(c, d, u.x), u.y);
}

fn value_fbm_2d(p: vec2<f32>) -> f32 {
    return value_noise_2d(p) * 0.6
        + value_noise_2d(p * 2.1 + vec2<f32>(4.3, 1.7)) * 0.4;
}

// Divergence-free curl of a scalar noise potential, producing flow without sources or sinks.
fn value_noise_curl_2d(p: vec2<f32>) -> vec2<f32> {
    let e = 0.5; // Central-difference step in noise-space units.
    return vec2<f32>(
         value_noise_2d(p + vec2<f32>(0.0, e)) - value_noise_2d(p - vec2<f32>(0.0, e)),
        -(value_noise_2d(p + vec2<f32>(e, 0.0)) - value_noise_2d(p - vec2<f32>(e, 0.0)))
    );
}
