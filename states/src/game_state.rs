use bevy::prelude::*;
use serde::Serialize;

use game_core::prelude::*;
use logging::prelude::*;

#[derive(Default, Clone, Copy, Debug, States, PartialEq, Eq, Hash, Serialize)]
pub enum GameState {
    #[default]
    Init,
    Running,
    Paused,
    Loading,
}

/// Run condition: true only in `Running` or `Paused`.
pub fn map_is_live(game_state: Option<Res<State<GameState>>>) -> bool {
    game_state.is_some_and(|game_state| matches!(game_state.get(), GameState::Running | GameState::Paused))
}

impl GameState {
    pub(crate) fn toggle_pause(
        mut commands: Commands,
        current_game_state: Res<State<GameState>>,
    ) {
        commands.trigger(SetGamePaused {
            paused: !matches!(current_game_state.get(), GameState::Paused),
            response: ResponseRequest::not_needed(),
        });
    }

    #[log_tags(Tag::Ui)]
    pub(crate) fn on_set_game_paused_do_so(
        trigger: On<SetGamePaused>,
        mut commands: Commands,
        mut next_game_state: ResMut<NextState<GameState>>,
        current_game_state: Res<State<GameState>>,
    ) {
        let requested_paused = trigger.paused;
        if let NextState::Pending(state) | NextState::PendingIfNeq(state) = *next_game_state {
            let result = SetGamePausedResult::OtherTransitionAlreadyQueued { state };
            trigger.response.report(&mut commands, |entity| SetGamePausedReport { entity, requested_paused, result });
            return;
        }
        let result = match (current_game_state.get(), requested_paused) {
            (GameState::Running, true) => {
                #[info_player("Game paused")]
                next_game_state.set(GameState::Paused);
                SetGamePausedResult::Applied
            }
            (GameState::Paused, false) => {
                #[info_player("Game resumed")]
                next_game_state.set(GameState::Running);
                SetGamePausedResult::Applied
            }
            (GameState::Running, false) | (GameState::Paused, true) => SetGamePausedResult::AlreadyInState,
            (state, _) => SetGamePausedResult::Rejected { state: *state },
        };
        trigger.response.report(&mut commands, |entity| SetGamePausedReport { entity, requested_paused, result });
    }
}

/// Requests `GameState::Paused` (`paused: true`) or `GameState::Running` (`paused: false`).
/// Rejected while the game is not in one of these states, or while another `GameState`
/// transition is already queued, so a pause never overrides a transition such as a map switch.
#[derive(Event)]
pub struct SetGamePaused {
    pub paused: bool,
    pub response: ResponseRequest,
}

#[derive(EntityEvent, Serialize)]
pub struct SetGamePausedReport {
    #[serde(skip)]
    pub entity: Entity,
    pub requested_paused: bool,
    pub result: SetGamePausedResult,
}

#[derive(Clone, Copy, Debug, Serialize)]
pub enum SetGamePausedResult {
    Applied,
    AlreadyInState,
    Rejected { state: GameState },
    OtherTransitionAlreadyQueued { state: GameState },
}
