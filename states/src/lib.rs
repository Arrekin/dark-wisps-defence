use bevy::{input::common_conditions::input_just_pressed, prelude::*};

mod game_state;
mod admin_mode;
mod ui_interaction;
mod map_loading_stage;

pub use game_state::{GameState, SetGamePaused, SetGamePausedReport, SetGamePausedResult, map_is_live};
pub use admin_mode::AdminMode;
pub use ui_interaction::UiInteraction;
pub use map_loading_stage::MapLoadingStage;

pub struct StatesPlugin;
impl Plugin for StatesPlugin {
    fn build(&self, app: &mut App) {
        app
            .init_state::<GameState>()
            .init_state::<UiInteraction>()
            .init_state::<MapLoadingStage>()
            .init_state::<AdminMode>()
            .add_observer(GameState::on_set_game_paused_do_so)
            .add_systems(PreUpdate, UiInteraction::on_escape.run_if(input_just_pressed(KeyCode::Escape)))
            .add_systems(Update, (
                GameState::toggle_pause.run_if(input_just_pressed(KeyCode::Space)),
                AdminMode::toggle_admin_mode.run_if(input_just_pressed(KeyCode::Tab)),
            ));
    }
}

pub mod prelude {
    pub use crate::{GameState, UiInteraction, MapLoadingStage};
}
