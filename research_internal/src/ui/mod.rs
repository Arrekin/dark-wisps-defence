use bevy::prelude::*;

pub(crate) mod action_button;
pub(crate) mod detail_view;
pub(crate) mod panel;
pub(crate) mod research_bar;
pub(crate) mod tile;

pub(crate) struct ResearchUiPlugin;
impl Plugin for ResearchUiPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            action_button::ResearchActionButtonPlugin,
            detail_view::ResearchDetailViewPlugin,
            panel::ResearchPanelPlugin,
            research_bar::ResearchBarPlugin,
            tile::ResearchTilePlugin,
        ));
    }
}
