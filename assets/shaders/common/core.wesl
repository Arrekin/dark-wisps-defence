const PI: f32 = 3.14159265359;
const TAU: f32 = 6.28318530718;

// World pixels per grid cell. Mirrors `game_core::grid::CELL_SIZE`.
const CELL_SIZE: f32 = 32.0;

// Mirrors `Bounds::contains` in `game_core::grid`.
fn bounds_contains(coords: vec2<i32>, bounds: vec2<u32>) -> bool {
    return all(coords >= vec2<i32>(0)) && all(coords < vec2<i32>(bounds));
}

// Row-major cell index; mirrors `Bounds::index` in `game_core::grid`. Only valid where
// `bounds_contains` holds for the same coords.
fn bounds_index(coords: vec2<i32>, bounds: vec2<u32>) -> u32 {
    return u32(coords.y) * bounds.x + u32(coords.x);
}
