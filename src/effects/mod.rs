pub mod explosions;
pub mod common;
pub mod wisp_attack;
pub mod ripple;
pub mod ripple_post_process;

use crate::prelude::*;

pub struct EffectsPlugin;
impl Plugin for EffectsPlugin {
    fn build(&self, app: &mut App) {
        app
            .add_plugins((
                explosions::ExplosionPlugin,
                wisp_attack::WispAttackEffectPlugin,
                ripple::RipplePlugin,
                ripple_post_process::RipplePostProcessPlugin,
            ))
            .add_systems(
            Update, (
                common::animate_sprite_system.run_if(in_state(GameState::Running)),
            ));
    }
}
