#define_import_path dwd::core

const PI: f32 = 3.14159265359;
const TAU: f32 = 6.28318530718;

// World pixels per grid cell; must match `game_core::CELL_SIZE`.
const CELL_SIZE: f32 = 32.0;

// Grid bounds helpers; mirror `Bounds::contains` and `Bounds::index` in `game_core::grid`.
fn grid_contains(coords: vec2<i32>, width: u32, height: u32) -> bool {
    return coords.x >= 0 && coords.y >= 0 && coords.x < i32(width) && coords.y < i32(height);
}

// Row-major cell index. Only valid where `grid_contains` holds for the same coords.
fn grid_index(coords: vec2<i32>, width: u32) -> u32 {
    return u32(coords.y) * width + u32(coords.x);
}
