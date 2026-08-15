use bevy::color::palettes::css::GRAY;
use bevy::prelude::*;

use game_core::prelude::*;

pub(crate) fn draw_grid_system(mut gizmos: Gizmos, map_info: Res<MapInfo>) {
    // Horizontal lines
    for y in 0..=map_info.grid_height {
        let start = Vec2::new(0.0, y as f32 * CELL_SIZE);
        let end = Vec2::new(map_info.world_width, y as f32 * CELL_SIZE);
        gizmos.line_2d(start, end, GRAY.with_alpha(0.05));
    }

    // Vertical lines
    for x in 0..=map_info.grid_width {
        let start = Vec2::new(x as f32 * CELL_SIZE, 0.0);
        let end = Vec2::new(x as f32 * CELL_SIZE, map_info.world_height);
        gizmos.line_2d(start, end, GRAY.with_alpha(0.05));
    }
}
