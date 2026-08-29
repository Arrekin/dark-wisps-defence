use bevy::color::palettes::css::GRAY;
use bevy::prelude::*;

use game_core::prelude::*;

pub(crate) fn draw_grid_system(mut gizmos: Gizmos, map_info: Res<MapInfo>) {
    let map_bounds = map_info.grid_bounds;
    let world_size = map_info.world_size();
    // Horizontal lines
    for y in 0..=map_bounds.height {
        let start = Vec2::new(0.0, y as f32 * CELL_SIZE);
        let end = Vec2::new(world_size.x, y as f32 * CELL_SIZE);
        gizmos.line_2d(start, end, GRAY.with_alpha(0.05));
    }

    // Vertical lines
    for x in 0..=map_bounds.width {
        let start = Vec2::new(x as f32 * CELL_SIZE, 0.0);
        let end = Vec2::new(x as f32 * CELL_SIZE, world_size.y);
        gizmos.line_2d(start, end, GRAY.with_alpha(0.05));
    }
}
