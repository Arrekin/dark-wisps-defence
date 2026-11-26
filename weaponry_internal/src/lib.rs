pub(crate) mod blast;
pub(crate) mod blaster_dart;
pub(crate) mod cannonball;
pub(crate) mod fireball;
pub(crate) mod flame_stream;
pub(crate) mod flame_stream_fire;
pub(crate) mod inferno_dome;
pub(crate) mod laser_beam;
pub(crate) mod lob;
pub(crate) mod lob_preview;
pub(crate) mod lob_preview_paths;
pub(crate) mod napalm;
pub(crate) mod napalm_bomb;
pub(crate) mod projectile_launch;
pub(crate) mod ripple;
pub(crate) mod rocket;

use bevy::prelude::*;

pub struct WeaponryPlugin;
impl Plugin for WeaponryPlugin {
    fn build(&self, app: &mut App) {
        app
            .add_plugins((
                blaster_dart::BlasterDartPlugin,
                projectile_launch::ProjectileLaunchPlugin,
                lob::LobPlugin,
                lob_preview::LobPreviewPlugin,
                lob_preview_paths::LobPreviewPathsPlugin,
                cannonball::CannonballPlugin,
                fireball::FireballPlugin,
                flame_stream::FlameStreamPlugin,
                flame_stream_fire::FlameStreamFirePlugin,
                inferno_dome::InfernoDomePlugin,
                laser_beam::LaserBeamPlugin,
                napalm::NapalmPlugin,
                napalm_bomb::NapalmBombPlugin,
                rocket::RocketPlugin,
                ripple::RipplePlugin,
            ));
    }
}
