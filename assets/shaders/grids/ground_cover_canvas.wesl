import bevy_sprite_render::mesh2d::vertex_output::VertexOutput;
import package::shaders::common::core::CELL_SIZE;
import package::shaders::common::map_info::grid_bounds;
import package::shaders::grids::ground_cover::{ground_cover_entries_at, ground_cover_masked_at};
import package::shaders::weaponry::napalm::{char_look, napalm_look};

// Draws ground cover in one pass over a quad covering the map.
// Blends domain shaders in `GroundCoverKind` order, bottom first, using premultiplied alpha.
// Walls and buildings hide ground cover on their cells.

@fragment
fn fragment(mesh: VertexOutput) -> @location(0) vec4<f32> {
    let f = mesh.uv * vec2<f32>(grid_bounds());
    let base = floor(f);
    let cell = vec2<i32>(base);
    if ground_cover_masked_at(cell) {
        return vec4<f32>(0.0);
    }
    let range = ground_cover_entries_at(cell);
    if range.x == range.y {
        return vec4<f32>(0.0);
    }

    let world = f * CELL_SIZE;
    let local = (f - base - vec2<f32>(0.5)) * CELL_SIZE;
    let char_mark = char_look(local, world, cell);
    let napalm = napalm_look(local, world, cell);
    return napalm + char_mark * (1.0 - napalm.a);
}
