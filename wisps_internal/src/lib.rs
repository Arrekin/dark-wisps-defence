pub(crate) mod face;
pub(crate) mod materials;
pub(crate) mod spawning;
pub(crate) mod systems;
pub(crate) mod summoning;
pub(crate) mod tooltip;

use bevy::prelude::*;
use bevy::sprite_render::Material2dPlugin;

use almanach::{WispInfo, prelude::*};
use game_core::{motion::MotionSystems, prelude::*};
use grids::placement::{annotate_non_empty, PlacementChannel, PlacementMode};
use persistence::prelude::{AppGameLoadSaveExtension, CollectSave};
use states::prelude::*;
use visuals::prelude::*;
use wisps::{BuilderWispFace, WispElectricType, WispFireType, WispLightType, WispWaterType, prelude::*};

pub struct WispsPlugin;
impl Plugin for WispsPlugin {
    fn build(&self, app: &mut App) {
        app
            .register_shader_library("shaders/wisps/fire_look.wgsl")
            .register_shader_library("shaders/wisps/water_look.wgsl")
            .register_shader_library("shaders/wisps/light_look.wgsl")
            .register_shader_library("shaders/wisps/electric_look.wgsl")
            .add_plugins((
                Material2dPlugin::<materials::WispFireMaterial>::default(),
                Material2dPlugin::<materials::WispWaterMaterial>::default(),
                Material2dPlugin::<materials::WispLightMaterial>::default(),
                Material2dPlugin::<materials::WispElectricMaterial>::default(),
            ))
            .add_plugins(summoning::SummoningPlugin)
            .add_plugins(face::WispFacePlugin)
            .add_plugins(tooltip::WispTooltipPlugin)
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
            // Feeds the freshly-tracked Locomotion into the motion-reactive wisp materials
            // before render extract; ordered after MotionSystems::Track so each reads this
            // frame's motion.
            .add_systems(PostUpdate, (
                systems::drive_water_material,
                systems::drive_wisp_locomotion::<materials::WispElectricMaterial>,
                systems::drive_wisp_locomotion::<materials::WispLightMaterial>,
                systems::drive_wisp_locomotion::<materials::WispFireMaterial>,
            ).after(MotionSystems::Track))
            .add_observer(spawning::BuilderWisp::on_builder_add_spawn_wisp)
            .add_observer(spawning::on_wisp_place_request_do_so)
            .add_observer(spawning::on_wisp_remove_request_do_so)
            .add_observer(spawning::on_wisp_spawn_attach_material::<WispFireType, materials::WispFireMaterial>)
            .add_observer(spawning::on_wisp_spawn_attach_material::<WispWaterType, materials::WispWaterMaterial>)
            .add_observer(spawning::on_wisp_spawn_attach_material::<WispLightType, materials::WispLightMaterial>)
            .add_observer(spawning::on_wisp_spawn_attach_material::<WispElectricType, materials::WispElectricMaterial>)
            .add_systems(CollectSave, spawning::collect_wisps)
            .register_loader(MapLoadingStage::SpawnMapElements, "wisps", spawning::load_wisps)
            .register_wisps(WispInfo {
                description: "A hostile wisp. Advances on your buildings and attacks what it reaches.".to_string(),
                grid_imprint: WISP_GRID_IMPRINT,
                validate: spawning::wisp_validator,
                annotate: annotate_non_empty,
                placement: PlacementChannel::of::<WispType>().with_modes(PlacementMode::OnPress),
                presentation: ObjectPresentation {
                    face: ObjectFace::Built(insert_wisp_face),
                    tooltip: Some(tooltip::wisp_tooltip),
                },
            });
    }
}

/// Inserts the UI face builder for a wisp placement tile.
fn insert_wisp_face(face_node: &mut EntityCommands, map_object: MapObject) {
    let MapObject::Wisp(wisp_type) = map_object else { return };
    face_node.insert(BuilderWispFace(wisp_type));
}