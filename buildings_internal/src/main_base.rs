use bevy::{
    platform::collections::HashMap,
    prelude::*,
};

use alteration::{
    effects::prelude::*,
    modifiers::prelude::*,
};
use almanach::prelude::*;
use buildings::prelude::*;
use game_core::prelude::*;
use grids::{
    emissions::{EmissionsType, EmitterEnergy, FloodEmissionsDetails, FloodEmissionsEvaluator, FloodEmissionsMode},
    energy_supply::{GeneratorEnergy, SupplierEnergy},
    placement::{annotate_non_empty, PlacementModes, PlaceRequest},
    prelude::ObstacleGridObject,
};
use persistence::{creating_new_map, prelude::*};
use states::prelude::*;
use viewport::MainCamera;

use crate::{common::*, tooltip::building_tooltip};

pub(crate) struct MainBasePlugin;
impl Plugin for MainBasePlugin {
    fn build(&self, app: &mut App) {
        let almanach_info = BuilderMainBase::almanach_info(app.world().resource::<AssetServer>());
        app
            .add_observer(BuilderMainBase::on_builder_add_spawn_main_base)
            .add_observer(on_main_base_place_request_do_so)
            .add_systems(CollectSave, collect_main_bases)
            .register_loader(MapLoadingStage::SpawnMapElements, "main_bases", load_main_bases)
            .add_systems(OnEnter(MapLoadingStage::SpawnMapElements), seed_main_base.run_if(creating_new_map))
            .add_systems(OnEnter(MapLoadingStage::Ready), center_camera_on_main_base)
            .register_building(BuildingType::MainBase, almanach_info);
    }
}

/// Spawn one `BuilderMainBase` at map center on a new map.
fn seed_main_base(mut commands: Commands, map_info: Res<MapInfo>) {
    let center = GridCoords {
        x: map_info.grid_bounds.width / 2,
        y: map_info.grid_bounds.height / 2,
    };
    commands.spawn(BuilderMainBase::new(center));
}

/// Runs at the end of every map build. Centers the main camera on the `MainBase`,
/// falling back to map center when there is no main base.
fn center_camera_on_main_base(
    map_info: Res<MapInfo>,
    main_base: Option<Single<&GlobalTransform, With<MainBase>>>,
    camera: Single<&mut Transform, With<MainCamera>>,
) {
    let center = match main_base {
        Some(base) => base.into_inner().translation().truncate(),
        None => map_info.world_size() / 2.0,
    };
    let translation = &mut camera.into_inner().translation;
    translation.x = center.x;
    translation.y = center.y;
}

#[derive(Component, SSS)]
pub(crate) struct BuilderMainBase {
    pub grid_position: GridCoords,
    pub integrity_points: Option<IntegrityPoints>,
}
impl BuilderMainBase {
    pub fn almanach_info(asset_server: &AssetServer) -> BuildingInfo {
        BuildingInfo {
            name: "Main Base".to_string(),
            description: "Home. Generates energy for the surrounding area.".to_string(),
            sprite: asset_server.load("buildings/main_base.png"),
            top_sprite: None,
            grid_imprint: GridImprint::Rectangle { width: 6, height: 6 },
            cost: vec![],
            baseline: HashMap::from([
                (ModifierType::MaxIntegrityPoints, 10000.),
                (ModifierType::EnergySupplyRange, 15.),
            ]),
            validate: building_validator,
            annotate: annotate_non_empty,
            placement: PlacementModes::default(),
            presentation: ObjectPresentation {
                tooltip: Some(building_tooltip),
            },
        }
    }

    pub fn new(grid_position: GridCoords) -> Self {
        Self { grid_position, integrity_points: None }
    }
    pub fn with_integrity_points(mut self, integrity_points: f32) -> Self {
        self.integrity_points = Some(IntegrityPoints::new(integrity_points));
        self
    }

    pub fn on_builder_add_spawn_main_base(
        trigger: On<Add, BuilderMainBase>,
        mut commands: Commands,
        almanach: Res<Almanach>,
        builders: Query<&BuilderMainBase>,
    ) {
        let entity = trigger.entity;
        let Ok(builder) = builders.get(entity) else { return; };

        let building_info = almanach.get_building_info(BuildingType::MainBase);
        let grid_imprint = building_info.grid_imprint;

        commands.entity(entity)
            .remove::<BuilderMainBase>()
            .insert_some(builder.integrity_points)
            .insert((
                Sprite {
                    image: building_info.sprite.clone(),
                    custom_size: Some(grid_imprint.world_size()),
                    ..default()
                },
                MainBase,
                builder.grid_position,
                grid_imprint,
                EmitterEnergy(FloodEmissionsDetails {
                    emissions_type: EmissionsType::Energy,
                    range: usize::MAX,
                    evaluator: FloodEmissionsEvaluator::ExponentialDecay { start_value: 100., decay: 0.1 },
                    mode: FloodEmissionsMode::Increase,
                }),
                GeneratorEnergy,
                SupplierEnergy,
                related![EffectInstances[
                    (ModifierContributions(building_info.baseline.clone()), BaselineEffect),
                ]],
            ))
            .observe(on_technical_state_changed_recompute_operational);
        commands.trigger(TechnicalStateChanged { entity, kind: TechnicalChange::JustSpawned });
    }
}

fn on_main_base_place_request_do_so(
    trigger: On<PlaceRequest>,
    mut commands: Commands,
    mut placement: BuildingPlacementManager,
    main_base: Single<Entity, With<MainBase>>,
) {
    let PlaceRequest(MapObject::Building(BuildingType::MainBase)) = *trigger else { return };
    let Some(coords) = placement.claim_free(BuildingType::MainBase) else { return };
    // Remove/Insert ObstacleGridObject to trigger grid reprint
    commands.entity(*main_base)
        .remove::<ObstacleGridObject>()
        .insert(coords)
        .insert(ObstacleGridObject::Building);
}

fn collect_main_bases(
    main_base: Single<(Entity, &GridCoords, &IntegrityPoints), With<MainBase>>,
    mut save: SaveWriter,
) {
    let (entity, &coords, integrity_points) = main_base.into_inner();
    let id = entity.index_u32() as i64;
    let integrity_points = integrity_points.get_current();
    save.submit(move |ctx| {
        ctx.save_marker("main_bases", id)?;
        ctx.save_grid_coords(id, coords)?;
        ctx.save_integrity_points(id, integrity_points)?;
        Ok(())
    });
}

fn load_main_bases(ctx: &mut LoadContext) -> LoadResult {
    ctx.for_each_entity("SELECT id FROM main_bases", |ctx, old_id, entity, _| {
        let grid_position = ctx.grid_coords(old_id)?;
        let integrity_points = ctx.integrity_points(old_id)?;
        let builder = BuilderMainBase::new(grid_position)
            .with_integrity_points(integrity_points);
        ctx.insert(entity, builder);
        Ok(())
    })
}
