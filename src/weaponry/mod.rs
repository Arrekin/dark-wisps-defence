pub mod laser_dart;
pub mod components;
pub mod cannonball;
pub mod rocket;
pub mod ripple;
pub mod ripple_post_process;
pub mod force_field;
pub mod force_field_post_process;


use crate::prelude::*;

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
