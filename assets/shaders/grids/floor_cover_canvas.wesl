import bevy_sprite_render::mesh2d::vertex_output::VertexOutput;
import package::shaders::common::core::CELL_SIZE;
import package::shaders::common::map_info::grid_bounds;
import package::shaders::grids::floor_cover::floor_cover_entries_at;
import package::shaders::weaponry::napalm::{char_look, napalm_look};

// Draws one floor's cover in one pass over a quad covering the map.
// Blends domain shaders in `FloorCoverKind` order, bottom first, using premultiplied alpha.

// Floor code (FLOOR_*) of the covers this canvas draws.
@group(2) @binding(0) var<uniform> floor_level: u32;

@fragment
fn fragment(mesh: VertexOutput) -> @location(0) vec4<f32> {
    let f = mesh.uv * vec2<f32>(grid_bounds());
    let base = floor(f);
    let cell = vec2<i32>(base);
    let range = floor_cover_entries_at(cell);
    if range.x == range.y {
        return vec4<f32>(0.0);
    }

    let world = f * CELL_SIZE;
    let local = (f - base - vec2<f32>(0.5)) * CELL_SIZE;
    let char_mark = char_look(local, world, cell, floor_level);
    let napalm = napalm_look(local, world, cell, floor_level);
    return napalm + char_mark * (1.0 - napalm.a);
}
