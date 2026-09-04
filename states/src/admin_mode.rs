use bevy::prelude::*;

use crate::game_state::GameState;

#[derive(Default, Clone, Copy, Debug, States, PartialEq, Eq, Hash)]
pub enum AdminMode {
    #[default]
    Disabled,
    Enabled,
}
impl AdminMode {
    pub fn is_enabled(&self) -> bool {
        matches!(self, AdminMode::Enabled)
    }
    pub(crate) fn toggle_admin_mode(
        mut next_admin_mode: ResMut<NextState<AdminMode>>,
        mut next_game_state: ResMut<NextState<GameState>>,
        current_admin_mode: Res<State<AdminMode>>,
    ) {
        // TODO: There is a risk of changing game state when loading etc.
        match current_admin_mode.get() {
            AdminMode::Disabled => {
                next_admin_mode.set(AdminMode::Enabled);
                next_game_state.set(GameState::Paused);
            },
            AdminMode::Enabled => {
                next_admin_mode.set(AdminMode::Disabled);
                next_game_state.set(GameState::Running);
            },
        }
    }
}
