#define_import_path dwd::voronoi_border
#import dwd::hash::{DWD_HASH_GOLDEN, dwd_hash_coords, dwd_hash_mix, dwd_hash_unit}

// Domain salt, so cell points stay independent of other consumers of the shared mixer.
const DWD_VORONOI_SALT: u32 = 0x165667b1u;

// Two deterministic values in [0, 1).
fn dwd_voronoi_hash_2d(p: vec2<f32>) -> vec2<f32> {
    let base = dwd_hash_coords(p, DWD_VORONOI_SALT);
    let second = dwd_hash_mix(base ^ DWD_HASH_GOLDEN);
    return vec2<f32>(dwd_hash_unit(base), dwd_hash_unit(second));
}

// Distance to the nearest Voronoi cell border: approximately zero on an edge and larger inside.
fn dwd_voronoi_border_2d(uv: vec2<f32>) -> f32 {
    let n = floor(uv);
    let f = fract(uv);
    var nearest = vec2<f32>(0.0);
    var distance = 8.0;
    for (var y = -1; y <= 1; y = y + 1) {
        for (var x = -1; x <= 1; x = x + 1) {
            let cell = vec2<f32>(f32(x), f32(y));
            let offset = cell + dwd_voronoi_hash_2d(n + cell) - f;
            let squared = dot(offset, offset);
            if squared < distance {
                distance = squared;
                nearest = offset;
            }
        }
    }
    distance = 8.0;
    for (var y = -1; y <= 1; y = y + 1) {
        for (var x = -1; x <= 1; x = x + 1) {
            let cell = vec2<f32>(f32(x), f32(y));
            let offset = cell + dwd_voronoi_hash_2d(n + cell) - f;
            let difference = offset - nearest;
            if dot(difference, difference) > 1e-5 {
                distance = min(distance, dot(0.5 * (nearest + offset), normalize(difference)));
            }
        }
    }
    return distance;
}
