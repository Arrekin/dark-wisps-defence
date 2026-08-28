#define_import_path dwd::hash

// Integer hashing for procedural detail: deterministic, uniformly distributed, and unaffected by
// coordinate magnitude.
//
// Callers fold in a domain salt, which keeps two consumers reading the same coordinate independent
// and stops an all-zero coordinate hashing to zero.

// Golden-ratio bit pattern, used to decorrelate integer hash inputs.
const DWD_HASH_GOLDEN: u32 = 0x9e3779b9u;

// Bijective bit mixer.
fn dwd_hash_mix(value: u32) -> u32 {
    var v = value;
    v = v ^ (v >> 16u);
    v = v * 0x7feb352du;
    v = v ^ (v >> 15u);
    v = v * 0x846ca68bu;
    v = v ^ (v >> 16u);
    return v;
}

// Maps the high 24 hash bits uniformly to representable f32 values in [0, 1).
fn dwd_hash_unit(value: u32) -> f32 {
    return f32(value >> 8u) * (1.0 / 16777216.0);
}

// Folds a two-component coordinate and a caller-chosen domain salt into one hash state. The
// asymmetric treatment of the two components keeps `(a, b)` and `(b, a)` apart.
fn dwd_hash_coords(coords: vec2<f32>, salt: u32) -> u32 {
    return dwd_hash_mix(salt ^ bitcast<u32>(coords.x) ^ (bitcast<u32>(coords.y) * DWD_HASH_GOLDEN));
}
