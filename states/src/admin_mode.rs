use bevy::prelude::*;
use serde::Serialize;

use game_core::prelude::*;

use crate::game_state::SetGamePaused;

#[derive(Default, Clone, Copy, Debug, States, PartialEq, Eq, Hash, Serialize)]
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
        mut commands: Commands,
        mut next_admin_mode: ResMut<NextState<AdminMode>>,
        current_admin_mode: Res<State<AdminMode>>,
    ) {
        match current_admin_mode.get() {
            AdminMode::Disabled => {
                next_admin_mode.set(AdminMode::Enabled);
                commands.trigger(SetGamePaused { paused: true, response: ResponseRequest::not_needed() });
            },
            AdminMode::Enabled => {
                next_admin_mode.set(AdminMode::Disabled);
                commands.trigger(SetGamePaused { paused: false, response: ResponseRequest::not_needed() });
            },
        }
    }
}
