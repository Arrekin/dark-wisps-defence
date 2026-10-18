use bevy::app::{App, Plugin};

pub(crate) mod game_clock;
pub(crate) mod moments;
pub(crate) mod stats;

pub struct SessionPlugin;
impl Plugin for SessionPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            game_clock::GameClockPlugin,
            stats::StatsPlugin,
            moments::SessionMomentsPlugin,
        ));
    }
}
