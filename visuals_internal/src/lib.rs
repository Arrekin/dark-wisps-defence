pub(crate) mod color_pulsation;
pub(crate) mod cut;
pub(crate) mod explosion;
pub(crate) mod render_schedule;
pub(crate) mod shader_clock;
pub(crate) mod shared_gpu;
pub(crate) mod sprite_animation;
pub(crate) mod void_sea;
pub(crate) mod wisp_attack;

use bevy::prelude::*;

use states::prelude::GameState;
use visuals::{canvas::UNIT_QUAD, drop_shadow::DropShadow, lane::LanePlugin};

pub struct VisualsPlugin;
impl Plugin for VisualsPlugin {
    fn build(&self, app: &mut App) {
        app
            .add_plugins((
                render_schedule::BandedCore2dPlugin,
                cut::CutPlugin,
                shared_gpu::SharedGpuPlugin,
                shader_clock::ShaderClockPlugin,
                explosion::ExplosionPlugin,
                void_sea::VoidSeaPlugin,
                wisp_attack::WispAttackEffectPlugin,
                LanePlugin::<DropShadow>::default(),
            ))
            .add_systems(Startup, |mut meshes: ResMut<Assets<Mesh>>| { meshes.insert(&UNIT_QUAD, Rectangle::new(1., 1.).into()).expect("a uuid mesh id is always valid"); })
            .add_systems(Update, (
                color_pulsation::pulsate_sprites,
                sprite_animation::animate_sprite.run_if(in_state(GameState::Running)),
            ))
            .add_observer(color_pulsation::on_remove_color_pulsation_reset_sprite_lightness);
    }
}
