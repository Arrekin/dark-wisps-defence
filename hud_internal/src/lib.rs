pub(crate) mod construction_menu;
pub(crate) mod badges;
pub(crate) mod main_menu;
pub(crate) mod pause_indicator;

pub(crate) mod display_info_panel;
pub(crate) mod grid_display;
pub(crate) mod indicators;

use bevy::prelude::*;

use hud::prelude::ShowGrid;

pub struct HudPlugin;
impl Plugin for HudPlugin {
    fn build(&self, app: &mut App) {
        app
            .add_plugins((
                badges::BadgesPlugin,
                display_info_panel::DisplayInfoPanelPlugin,
                indicators::IndicatorsPlugin,
                construction_menu::ConstructionMenuPlugin,
                main_menu::MainMenuPlugin,
                pause_indicator::PauseIndicatorPlugin,
            ))
            .add_systems(Update, grid_display::draw_grid_system.run_if(resource_exists::<ShowGrid>));

    }
}