pub(crate) mod common;
pub(crate) mod color_pulsation;
pub(crate) mod post_process;
pub(crate) mod explosion;
pub(crate) mod wisp_attack;

use bevy::prelude::*;
use states::prelude::GameState;

pub struct VisualsPlugin;
impl Plugin for VisualsPlugin {
    fn build(&self, app: &mut App) {
        app
            .add_systems(Update, (
                color_pulsation::pulsate_sprites_system,
            ))
            .add_observer(color_pulsation::on_remove_color_pulsation_reset_sprite_lightness)
            .add_plugins(post_process::PostProcessOrderingPlugin)
            .add_plugins(explosion::ExplosionPlugin)
            .add_plugins(wisp_attack::WispAttackEffectPlugin)
            .add_systems(
                Update,
                common::animate_sprite_system.run_if(in_state(GameState::Running)),
            );
    }
}
