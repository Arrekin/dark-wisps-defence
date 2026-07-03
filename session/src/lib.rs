use bevy::app::{App, Plugin};

pub mod game_clock;
pub mod stats;

pub use game_clock::GameClock;
pub use stats::StatsWispsKilled;

pub struct SessionPlugin;
impl Plugin for SessionPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            game_clock::GameClockPlugin,
            stats::StatsPlugin,
        ));
    }
}
