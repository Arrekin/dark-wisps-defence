use bevy::prelude::*;

#[derive(Default, Clone, Copy, Debug, States, PartialEq, Eq, Hash)]
pub enum GameState {
    #[default]
    Init,
    Running,
    Paused,
    Loading,
}
impl GameState {
    pub(crate) fn pause_resume_game(
        mut next_game_state: ResMut<NextState<GameState>>,
        current_game_state: Res<State<GameState>>
    ) {
        match current_game_state.get() {
            GameState::Init => {}
            GameState::Paused => next_game_state.set(GameState::Running),
            GameState::Running => next_game_state.set(GameState::Paused),
            GameState::Loading => {}
        }
    }
}
