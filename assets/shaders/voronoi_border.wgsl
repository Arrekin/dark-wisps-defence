#define_import_path dwd::voronoi_border

fn dwd_voronoi_hash_2d(p: vec2<f32>) -> vec2<f32> {
    let k = vec2<f32>(
        dot(p, vec2<f32>(127.1, 311.7)),
        dot(p, vec2<f32>(269.5, 183.3)),
    );
    return fract(sin(k) * 43758.5453123);
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
