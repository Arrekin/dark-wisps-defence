pub mod components;
mod materials;
pub mod spawning;
pub mod systems;
pub mod summoning;

use bevy::sprite_render::Material2dPlugin;
use lib_core::wisps::{WispFireType, WispWaterType, WispLightType, WispElectricType};
use lib_inventory::almanach::WispInfo;
use lib_inventory::placement::annotate_non_empty;

use crate::prelude::*;
use crate::visual_effects::effect_material::sync_effect_visuals;

pub struct WispsPlugin;
impl Plugin for WispsPlugin {
    fn build(&self, app: &mut App) {
        app
            .add_plugins((
                Material2dPlugin::<materials::WispFireMaterial>::default(),
                Material2dPlugin::<materials::WispWaterMaterial>::default(),
                Material2dPlugin::<materials::WispLightMaterial>::default(),
                Material2dPlugin::<materials::WispElectricMaterial>::default(),
                
            ))
            .add_plugins(summoning::SummoningPlugin)
            .add_systems(PreUpdate,
                systems::remove_dead_wisps.run_if(in_state(GameState::Running)),
            )
            .add_systems(Update, (
                (
                    systems::move_wisps,
                    systems::target_wisps,
                    systems::wisp_charge_attack,
                    systems::collide_wisps,
                ).run_if(in_state(GameState::Running)),
            ))
            .add_systems(Update, (
                sync_effect_visuals::<materials::WispFireMaterial>,
                sync_effect_visuals::<materials::WispWaterMaterial>,
                sync_effect_visuals::<materials::WispLightMaterial>,
                sync_effect_visuals::<materials::WispElectricMaterial>,
            ))
            .add_observer(spawning::BuilderWisp::on_add)
            .add_observer(spawning::on_wisp_place_request)
            .add_observer(spawning::on_wisp_remove_request)
            .add_observer(spawning::on_wisp_spawn_attach_material::<WispFireType, materials::WispFireMaterial>)
            .add_observer(spawning::on_wisp_spawn_attach_material::<WispWaterType, materials::WispWaterMaterial>)
            .add_observer(spawning::on_wisp_spawn_attach_material::<WispLightType, materials::WispLightMaterial>)
            .add_observer(spawning::on_wisp_spawn_attach_material::<WispElectricType, materials::WispElectricMaterial>)
            .register_db_loader::<spawning::BuilderWisp>(MapLoadingStage::SpawnMapElements)
            .register_db_saver(spawning::BuilderWisp::on_game_save)
            .register_wisps(WispInfo {
                grid_imprint: spawning::WISP_GRID_IMPRINT,
                validate: spawning::wisp_validator,
                annotate: annotate_non_empty,
            });
    }
}