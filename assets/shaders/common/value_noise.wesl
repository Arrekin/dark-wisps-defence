import package::shaders::common::hash::{HASH_GOLDEN, hash_coords, hash_mix, hash_unit};

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

// The third axis folds into the 2D hash, so its lattice stays independent of the first two.
fn value_hash_3d(p: vec3<f32>) -> f32 {
    return hash_unit(hash_mix(hash_coords(p.xy, VALUE_SALT) ^ (bitcast<u32>(p.z) * HASH_GOLDEN)));
}

fn value_noise_3d(p: vec3<f32>) -> f32 {
    let i = floor(p);
    let f = fract(p);
    let u = f * f * (3.0 - 2.0 * f);
    let bottom = mix(
        mix(value_hash_3d(i), value_hash_3d(i + vec3<f32>(1.0, 0.0, 0.0)), u.x),
        mix(value_hash_3d(i + vec3<f32>(0.0, 1.0, 0.0)), value_hash_3d(i + vec3<f32>(1.0, 1.0, 0.0)), u.x),
        u.y,
    );
    let top = mix(
        mix(value_hash_3d(i + vec3<f32>(0.0, 0.0, 1.0)), value_hash_3d(i + vec3<f32>(1.0, 0.0, 1.0)), u.x),
        mix(value_hash_3d(i + vec3<f32>(0.0, 1.0, 1.0)), value_hash_3d(i + vec3<f32>(1.0, 1.0, 1.0)), u.x),
        u.y,
    );
    return mix(bottom, top, u.z);
}

// Normalized by the summed amplitudes, so the output stays in [0, 1] with its mean near 0.5 for any
// octave count. Each octave shifts as well as doubles, so the lattices do not line up.
fn value_fbm_3d(p: vec3<f32>, octaves: i32) -> f32 {
    var total = 0.0;
    var amplitude = 0.5;
    var norm = 0.0;
    var point = p;
    for (var i = 0; i < octaves; i++) {
        total = total + amplitude * value_noise_3d(point);
        norm = norm + amplitude;
        point = point * 2.03 + vec3<f32>(1.7, 9.2, 3.1);
        amplitude = amplitude * 0.5;
    }
    return total / norm;
}
