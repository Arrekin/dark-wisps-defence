pub(crate) mod laser_dart;
pub(crate) mod cannonball;
pub(crate) mod fireball;
pub(crate) mod flame_stream;
pub(crate) mod flame_stream_fire;
pub(crate) mod inferno_dome;
pub(crate) mod inferno_dome_post_process;
pub(crate) mod lob;
pub(crate) mod rocket;
pub(crate) mod ripple;
pub(crate) mod ripple_post_process;
pub(crate) mod force_field;
pub(crate) mod force_field_post_process;

use bevy::prelude::*;

pub struct WeaponryPlugin;
impl Plugin for WeaponryPlugin {
    fn build(&self, app: &mut App) {
        app
            .add_plugins((
                laser_dart::LaserDartPlugin,
                lob::LobPlugin,
                cannonball::CannonballPlugin,
                fireball::FireballPlugin,
                flame_stream::FlameStreamPlugin,
                flame_stream_fire::FlameStreamFirePlugin,
                inferno_dome::InfernoDomePlugin,
                inferno_dome_post_process::InfernoDomePostProcessPlugin,
                rocket::RocketPlugin,
                ripple::RipplePlugin,
                ripple_post_process::RipplePostProcessPlugin,
                force_field::ForceFieldPlugin,
                force_field_post_process::ForceFieldPostProcessPlugin,
            ));
    }
}
