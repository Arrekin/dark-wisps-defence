pub(crate) mod cannonball;
pub(crate) mod fireball;
pub(crate) mod flame_stream;
pub(crate) mod flame_stream_fire;
pub(crate) mod inferno_dome;
pub(crate) mod laser_dart;
pub(crate) mod lob;
pub(crate) mod napalm;
pub(crate) mod napalm_bomb;
pub(crate) mod ripple;
pub(crate) mod rocket;

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
                napalm::NapalmPlugin,
                napalm_bomb::NapalmBombPlugin,
                rocket::RocketPlugin,
                ripple::RipplePlugin,
            ));
    }
}
