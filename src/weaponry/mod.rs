pub mod laser_dart;
pub mod components;
pub mod cannonball;
pub mod rocket;
pub mod ripple;
pub mod ripple_post_process;


use crate::prelude::*;

pub struct ProjectilesPlugin;
impl Plugin for ProjectilesPlugin {
    fn build(&self, app: &mut App) {
        app
            .add_plugins((
                laser_dart::LaserDartPlugin,
                cannonball::CannonballPlugin,
                rocket::RocketPlugin,
                ripple::RipplePlugin,
                ripple_post_process::RipplePostProcessPlugin,
            ));

    }
}
