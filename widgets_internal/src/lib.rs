use bevy::app::{App, Plugin};

pub(crate) mod healthbar;
pub(crate) mod chips;
pub(crate) mod close_button;
pub(crate) mod fill_bar;
pub(crate) mod text_command_button;
pub(crate) mod tooltip;
pub(crate) mod typography;
pub(crate) mod mouse_scrolling;
pub(crate) mod progress_bar;
pub(crate) mod rune;
pub(crate) mod void_panel;

pub struct WidgetsPlugin;
impl Plugin for WidgetsPlugin {
    fn build(&self, app: &mut App) {
        app
            .add_plugins((
                healthbar::HealthbarPlugin,
                chips::ChipsPlugin,
                close_button::CloseButtonPlugin,
                fill_bar::FillBarPlugin,
                text_command_button::TextCommandButtonPlugin,
                tooltip::TooltipPlugin,
                typography::TypographyPlugin,
                mouse_scrolling::MouseScrollingPlugin,
                progress_bar::ProgressBarPlugin,
                rune::RunePlugin,
                void_panel::VoidPanelPlugin,
            ));
    }
}
