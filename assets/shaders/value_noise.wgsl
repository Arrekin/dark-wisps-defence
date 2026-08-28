#define_import_path dwd::value_noise

fn dwd_value_hash_2d(p: vec2<f32>) -> f32 {
    let h = dot(p, vec2<f32>(127.1, 311.7));
    return fract(sin(h) * 43758.5453123);
}

fn dwd_value_noise_2d(p: vec2<f32>) -> f32 {
    let i = floor(p);
    let f = fract(p);
    let a = dwd_value_hash_2d(i);
    let b = dwd_value_hash_2d(i + vec2<f32>(1.0, 0.0));
    let c = dwd_value_hash_2d(i + vec2<f32>(0.0, 1.0));
    let d = dwd_value_hash_2d(i + vec2<f32>(1.0, 1.0));
    let u = f * f * (3.0 - 2.0 * f);
    return mix(mix(a, b, u.x), mix(c, d, u.x), u.y);
}

fn dwd_value_fbm_2d(p: vec2<f32>) -> f32 {
    return dwd_value_noise_2d(p) * 0.6
        + dwd_value_noise_2d(p * 2.1 + vec2<f32>(4.3, 1.7)) * 0.4;
}

// Divergence-free curl of a scalar noise potential, producing flow without sources or sinks.
fn dwd_value_noise_curl_2d(p: vec2<f32>) -> vec2<f32> {
    let e = 0.5; // Central-difference step in noise-space units.
    return vec2<f32>(
         dwd_value_noise_2d(p + vec2<f32>(0.0, e)) - dwd_value_noise_2d(p - vec2<f32>(0.0, e)),
        -(dwd_value_noise_2d(p + vec2<f32>(e, 0.0)) - dwd_value_noise_2d(p - vec2<f32>(e, 0.0)))
    );
}
