use bevy::prelude::*;

#[derive(Default, Clone, Debug, States, PartialEq, Eq, Hash)]
pub enum UiInteraction {
    #[default]
    Free, // No interaction
    MainMenu,
    PlaceGridObject,
    DisplayInfoPanel,
    ResearchPanel,
}
impl UiInteraction {
    // On ESC: if UI is free, open Main Menu; otherwise, return to Free
    pub(crate) fn on_escape(
        mut next_ui_state: ResMut<NextState<UiInteraction>>,
        current_ui_state: Res<State<UiInteraction>>
    ) {
        match current_ui_state.get() {
            UiInteraction::Free => next_ui_state.set(UiInteraction::MainMenu),
            _ => next_ui_state.set(UiInteraction::Free),
        }
    }
}
