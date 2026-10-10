use bevy::prelude::*;
use serde::Serialize;

#[derive(Default, Clone, Debug, States, PartialEq, Eq, Hash, Serialize)]
pub enum UiInteraction {
    #[default]
    /// No UI interaction is in progress.
    Free,
    MainMenu,
    PlaceGridObject,
    DisplayInfoPanel,
    ResearchPanel,
    ForgingPanel,
}
impl UiInteraction {
    /// On Escape: opens the main menu from `Free`, otherwise returns to `Free`.
    pub(crate) fn on_escape(
        mut next_ui_state: ResMut<NextState<UiInteraction>>,
        current_ui_state: Res<State<UiInteraction>>,
    ) {
        match current_ui_state.get() {
            UiInteraction::Free => next_ui_state.set(UiInteraction::MainMenu),
            _ => next_ui_state.set(UiInteraction::Free),
        }
    }
}
