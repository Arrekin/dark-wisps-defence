use bevy::app::{App, Plugin};

pub mod game_clock;
pub mod start_game_trigger;
pub mod stats;

pub use game_clock::GameClock;
pub use start_game_trigger::TriggerStartGame;
pub use stats::StatsWispsKilled;

pub struct SessionPlugin;
impl Plugin for SessionPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            game_clock::GameClockPlugin,
            stats::StatsPlugin,
            start_game_trigger::StartGameTriggerPlugin,
        ));
    }
}
