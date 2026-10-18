pub(crate) mod color_pulsation;
pub(crate) mod cut;
pub(crate) mod explosion;
pub(crate) mod map_ground;
pub(crate) mod render_schedule;
pub(crate) mod shared_gpu;
pub(crate) mod sprite_animation;
pub(crate) mod wisp_attack;

use bevy::prelude::*;

use states::prelude::GameState;
use visuals::canvas::UNIT_QUAD;

pub struct VisualsPlugin;
impl Plugin for VisualsPlugin {
    fn build(&self, app: &mut App) {
        app
            .add_plugins((
                render_schedule::BandedCore2dPlugin,
                cut::CutPlugin,
                shared_gpu::SharedGpuPlugin,
                explosion::ExplosionPlugin,
                map_ground::MapGroundPlugin,
                wisp_attack::WispAttackEffectPlugin,
            ))
            .add_systems(Startup, |mut meshes: ResMut<Assets<Mesh>>| { meshes.insert(&UNIT_QUAD, Rectangle::new(1., 1.).into()).expect("a uuid mesh id is always valid"); })
            .add_systems(Update, (
                color_pulsation::pulsate_sprites_system,
                sprite_animation::animate_sprite_system.run_if(in_state(GameState::Running)),
            ))
            .add_observer(color_pulsation::on_remove_color_pulsation_reset_sprite_lightness);
    }
}
