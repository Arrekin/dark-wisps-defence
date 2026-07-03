use bevy::app::{App, Plugin};

pub(crate) mod healthbar;
pub(crate) mod cost_indicator;
pub(crate) mod tooltip;
pub(crate) mod mouse_scrolling;

pub struct WidgetsPlugin;
impl Plugin for WidgetsPlugin {
    fn build(&self, app: &mut App) {
        app
            .add_plugins((
                healthbar::HealthbarPlugin,
                cost_indicator::CostIndicatorPlugin,
                tooltip::TooltipPlugin,
                mouse_scrolling::MouseScrollingPlugin,
            ));
    }
}
