pub(crate) mod face;
pub(crate) mod looks;
pub(crate) mod spawning;
pub(crate) mod summoning;
pub(crate) mod systems;
pub(crate) mod tooltip;

use bevy::prelude::*;

use almanach::{ObjectPresentation, WispInfo, prelude::*};
use game_core::motion::MotionSystems;
use grids::placement::{annotate_non_empty, PlacementModes};
use persistence::prelude::{AppGameLoadSaveExtension, CollectSave};
use states::prelude::*;
use visuals::{effect_visuals::sync_effect_visuals, lane::LanePlugin};
use wisps::prelude::*;

pub struct WispsPlugin;
impl Plugin for WispsPlugin {
    fn build(&self, app: &mut App) {
        app
            .add_plugins((
                LanePlugin::<looks::WispFireVisual>::default(),
                LanePlugin::<looks::WispWaterVisual>::default(),
                LanePlugin::<looks::WispLightVisual>::default(),
                LanePlugin::<looks::WispElectricVisual>::default(),
            ))
            .add_plugins((
                summoning::SummoningPlugin,
                face::WispFacePlugin,
                tooltip::WispTooltipPlugin,
            ))
            .add_systems(PreUpdate, systems::remove_dead_wisps.run_if(in_state(GameState::Running)))
            .add_systems(Update, (
                systems::move_wisps,
                systems::target_wisps,
                systems::wisp_charge_attack,
                systems::collide_wisps,
            ).run_if(in_state(GameState::Running)))
            .add_systems(Update, (
                sync_effect_visuals::<looks::WispFireVisual>,
                sync_effect_visuals::<looks::WispWaterVisual>,
                sync_effect_visuals::<looks::WispLightVisual>,
                sync_effect_visuals::<looks::WispElectricVisual>,
            ))
            // Read locomotion after tracking so visuals use this frame's motion.
            // before render extract; ordered after MotionSystems::Track so each reads this
            // frame's motion.
            .add_systems(PostUpdate, (
                systems::drive_wisp_water_look,
                systems::drive_wisp_motion_look::<looks::WispElectricVisual>,
                systems::drive_wisp_motion_look::<looks::WispLightVisual>,
                systems::drive_wisp_motion_look::<looks::WispFireVisual>,
            ).after(MotionSystems::Track))
            .add_observer(spawning::BuilderWisp::on_builder_add_spawn_wisp)
            .add_observer(spawning::on_wisp_place_request_do_so)
            .add_observer(spawning::on_wisp_remove_request_do_so)
            .add_systems(CollectSave, spawning::collect_wisps)
            .register_loader(MapLoadingStage::SpawnMapElements, "wisps", spawning::load_wisps)
            .register_wisps(WispInfo {
                description: "A hostile wisp. Advances on your buildings and attacks what it reaches.".to_string(),
                grid_imprint: WISP_GRID_IMPRINT,
                validate: spawning::wisp_validator,
                annotate: annotate_non_empty,
                placement: PlacementModes::on_press(),
                presentation: ObjectPresentation {
                    tooltip: Some(tooltip::wisp_tooltip),
                },
            });
    }
}
