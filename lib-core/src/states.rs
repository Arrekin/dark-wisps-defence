use bevy::input::common_conditions::input_just_pressed;

use crate::lib_prelude::*;

pub mod states_prelude {
    pub use super::*;
}

pub struct StatesPlugin;
impl Plugin for StatesPlugin {
    fn build(&self, app: &mut App) {
        app
            .init_state::<GameState>()
            .init_state::<UiInteraction>()
            .init_state::<MapLoadingStage>()
            .init_state::<AdminMode>()
            .add_systems(PreUpdate, (
                UiInteraction::on_escape.run_if(input_just_pressed(KeyCode::Escape)),
            ))
            .add_systems(Update, (
                GameState::pause_resume_game.run_if(input_just_pressed(KeyCode::Space)),
                AdminMode::toggle_admin_mode.run_if(input_just_pressed(KeyCode::Tab)),
            ));
    }
}

#[derive(Default, Clone, Copy, Debug, States, PartialEq, Eq, Hash)]
pub enum GameState {
    #[default]
    Init,
    Running,
    Paused,
    Loading,
}
impl GameState {
    fn pause_resume_game(
        current_game_state: Res<State<GameState>>,
        mut next_game_state: ResMut<NextState<GameState>>
    ) {
        match current_game_state.get() {
            GameState::Init => {}
            GameState::Paused => next_game_state.set(GameState::Running),
            GameState::Running => next_game_state.set(GameState::Paused),
            GameState::Loading => {}
        }
    }
}

#[derive(Default, Clone, Copy, Debug, States, PartialEq, Eq, Hash)]
pub enum AdminMode {
    #[default]
    Disabled,
    Enabled,
}
impl AdminMode {
    fn toggle_admin_mode(
        current_admin_mode: Res<State<AdminMode>>,
        mut next_admin_mode: ResMut<NextState<AdminMode>>,
        mut next_game_state: ResMut<NextState<GameState>>,
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
    fn on_escape(
        current_ui_state: Res<State<UiInteraction>>, 
        mut next_ui_state: ResMut<NextState<UiInteraction>>
    ) {
        match current_ui_state.get() {
            UiInteraction::Free => next_ui_state.set(UiInteraction::MainMenu),
            _ => next_ui_state.set(UiInteraction::Free),
        }
    }
}

#[derive(Default, Clone, Debug, States, PartialEq, Eq, Hash)]
pub enum MapLoadingStage {
    #[default]
    Init,
    LoadMapInfo,
    LoadResources,
    SpawnMapElements,
    SpawnEffectInstances,
    Ready,
}
impl MapLoadingStage {
    pub fn next(&self) -> Option<Self> {
        match self {
            MapLoadingStage::Init => Some(MapLoadingStage::LoadMapInfo),
            MapLoadingStage::LoadMapInfo => Some(MapLoadingStage::LoadResources),
            MapLoadingStage::LoadResources => Some(MapLoadingStage::SpawnMapElements),
            MapLoadingStage::SpawnMapElements => Some(MapLoadingStage::SpawnEffectInstances),
            MapLoadingStage::SpawnEffectInstances => Some(MapLoadingStage::Ready),
            MapLoadingStage::Ready => None,
        }
    }
}