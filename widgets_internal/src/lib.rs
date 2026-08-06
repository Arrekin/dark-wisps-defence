use bevy::app::{App, Plugin};

pub(crate) mod healthbar;
pub(crate) mod chips;
pub(crate) mod fill_bar;
pub(crate) mod tooltip;
pub(crate) mod mouse_scrolling;

pub struct WidgetsPlugin;
impl Plugin for WidgetsPlugin {
    fn build(&self, app: &mut App) {
        app
            .add_plugins((
                healthbar::HealthbarPlugin,
                chips::ChipsPlugin,
                fill_bar::FillBarPlugin,
                tooltip::TooltipPlugin,
                mouse_scrolling::MouseScrollingPlugin,
            ));
    }
}
