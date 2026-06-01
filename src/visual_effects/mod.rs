pub mod explosions;
pub mod common;
pub mod wisp_attack;
pub mod effect_material;

use crate::prelude::*;

pub struct VisualEffectsPlugin;
impl Plugin for VisualEffectsPlugin {
    fn build(&self, app: &mut App) {
        app
            .add_plugins((
                explosions::ExplosionPlugin,
                wisp_attack::WispAttackEffectPlugin,
            ))
            .add_systems(
            Update, (
                common::animate_sprite_system.run_if(in_state(GameState::Running)),
            ));
    }
}
