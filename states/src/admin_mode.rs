use bevy::prelude::*;
use serde::Serialize;

use game_core::prelude::*;
use logging::prelude::*;

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
    #[log_tags(Tag::Editor)]
    pub(crate) fn toggle_admin_mode(
        mut commands: Commands,
        mut next_admin_mode: ResMut<NextState<AdminMode>>,
        current_admin_mode: Res<State<AdminMode>>,
    ) {
        match current_admin_mode.get() {
            AdminMode::Disabled => {
                #[info_player("Admin mode enabled")]
                next_admin_mode.set(AdminMode::Enabled);
                commands.trigger(SetGamePaused { paused: true, response: ResponseRequest::not_needed() });
            }
            AdminMode::Enabled => {
                #[info_player("Admin mode disabled")]
                next_admin_mode.set(AdminMode::Disabled);
                commands.trigger(SetGamePaused { paused: false, response: ResponseRequest::not_needed() });
            }
        }
    }
}
