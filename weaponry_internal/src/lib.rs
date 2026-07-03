pub(crate) mod laser_dart;
pub(crate) mod cannonball;
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
                cannonball::CannonballPlugin,
                rocket::RocketPlugin,
                ripple::RipplePlugin,
                ripple_post_process::RipplePostProcessPlugin,
                force_field::ForceFieldPlugin,
                force_field_post_process::ForceFieldPostProcessPlugin,
            ));

    }
}
