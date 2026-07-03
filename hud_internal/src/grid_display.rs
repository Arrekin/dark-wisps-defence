use bevy::color::palettes::css::GRAY;
use bevy::prelude::*;

use game_core::prelude::*;
use grids::prelude::ObstacleGrid;

use crate::UiConfig;

pub(crate) fn show_hide_grid_system(keys: Res<ButtonInput<KeyCode>>, mut ui_config: ResMut<UiConfig>) {
    if keys.just_pressed(KeyCode::KeyG) {
        ui_config.show_grid = !ui_config.show_grid;
    }
}

pub(crate) fn draw_grid_system(mut gizmos: Gizmos, grid: Res<ObstacleGrid>, ui_config: Res<UiConfig>) {
    if !ui_config.show_grid { return; }

    let total_height = grid.height as f32 * CELL_SIZE;
    let total_width = grid.width as f32 * CELL_SIZE;

    // Horizontal lines
    for y in 0..=grid.height {
        let start = Vec2::new(0.0, y as f32 * CELL_SIZE);
        let end = Vec2::new(total_width, y as f32 * CELL_SIZE);
        gizmos.line_2d(start, end, GRAY.with_alpha(0.05));
    }

    // Vertical lines
    for x in 0..=grid.width {
        let start = Vec2::new(x as f32 * CELL_SIZE, 0.0);
        let end = Vec2::new(x as f32 * CELL_SIZE, total_height);
        gizmos.line_2d(start, end, GRAY.with_alpha(0.05));
    }
}
