use bevy::{
    ecs::system::SystemParam,
    platform::collections::HashMap,
    prelude::*,
};

use almanach::{BuildingInfo, prelude::*};
use alteration::{
    effects::prelude::*,
    modifiers::prelude::*,
};
use buildings::prelude::*;
use game_core::prelude::*;
use grids::placement::{PlacementFeedback, PlacementObject, PlacementSession, PlacementValidity, PlacementFeedbackSystems, PlaceRequest, ReservedCoords};
use indexes::{
    emissions::{EmissionsType, EmitterEnergy, FloodEmissionsDetails, FloodEmissionsEvaluator, FloodEmissionsMode},
    energy_supply::{GeneratorEnergy, SupplierEnergy},
    prelude::{ObstacleCell, ObstacleGrid},
};
use logging::prelude::*;
use persistence::{creating_new_map, prelude::*};
use states::prelude::*;
use viewport::MainCamera;

use crate::common::*;

pub(crate) struct MainBasePlugin;
impl Plugin for MainBasePlugin {
    fn build(&self, app: &mut App) {
        let almanach_info = BuilderMainBase::almanach_info(app.world().resource::<AssetServer>());
        app
            .add_systems(Update, MainBasePlacementRules::update_feedback.in_set(PlacementFeedbackSystems))
            .add_observer(BuilderMainBase::on_builder_add_spawn_main_base)
            .add_observer(MainBasePlacementRules::on_place_request_do_so)
            .add_systems(CollectSave, collect_main_bases)
            .register_loader(MapLoadingStage::SpawnMapElements, "main_bases", load_main_bases)
            .add_systems(OnEnter(MapLoadingStage::SpawnMapElements), seed_main_base.run_if(creating_new_map))
            .add_systems(OnEnter(MapLoadingStage::Ready), center_camera_on_main_base)
            .register_building(BuildingType::MainBase, almanach_info);
    }
}

/// Spawn one `BuilderMainBase` at map center on a new map.
fn seed_main_base(mut commands: Commands, almanach: Res<Almanach>, map_info: Res<MapInfo>) {
    let center = GridCoords {
        x: map_info.grid_bounds.width / 2,
        y: map_info.grid_bounds.height / 2,
    };
    let imprint = almanach.get_building_info(BuildingType::MainBase).grid_imprint;
    commands.spawn(BuilderMainBase::new(GridArea::centered_on(center.at(AltitudeLevel::Elevation), imprint).origin));
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
    *translation = center.extend(translation.z);
}

#[derive(Component, SSS)]
pub(crate) struct BuilderMainBase {
    pub grid_position: GridPosition,
    pub integrity_points: Option<IntegrityPoints>,
}
impl BuilderMainBase {
    pub fn almanach_info(asset_server: &AssetServer) -> BuildingInfo {
        BuildingInfo {
            name: "Main Base".to_string(),
            description: "Home. Generates energy for the surrounding area.".to_string(),
            sprite: asset_server.load("buildings/main_base.png"),
            top_sprite: None,
            grid_imprint: GridImprint::square(6),
            attack_range: None,
            cost: vec![],
            baseline: HashMap::from([
                (ModifierType::MaxIntegrityPoints, 10000.),
                (ModifierType::EnergySupplyRange, 15.),
            ]),
            sockets: vec![],
        }
    }

    pub fn new(grid_position: GridPosition) -> Self {
        Self { grid_position, integrity_points: None }
    }
    pub fn with_integrity_points(mut self, integrity_points: f32) -> Self {
        self.integrity_points = Some(IntegrityPoints::new(integrity_points));
        self
    }

    pub fn on_builder_add_spawn_main_base(
        trigger: On<Add<BuilderMainBase>>,
        mut commands: Commands,
        almanach: Res<Almanach>,
        builders: Query<&BuilderMainBase>,
    ) {
        let entity = trigger.entity;
        let Ok(builder) = builders.get(entity) else { return; };

        let building_info = almanach.get_building_info(BuildingType::MainBase);

        commands.entity(entity)
            .remove::<BuilderMainBase>()
            .insert_some(builder.integrity_points)
            .insert((
                building_info.world_sprite(),
                MainBase,
                GridArea::new(builder.grid_position, building_info.grid_imprint),
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

/// Validates and moves the existing main base to the session's target area.
#[derive(SystemParam)]
pub(crate) struct MainBasePlacementRules<'w, 's> {
    commands: Commands<'w, 's>,
    obstacle_grid: ResMut<'w, ObstacleGrid>,
    reserved_coords: ResMut<'w, ReservedCoords>,
    placement_session: Single<'w, 's, &'static GridArea, With<PlacementSession>>,
    main_base: Single<'w, 's, (Entity, &'static GridArea), With<MainBase>>,
}
impl MainBasePlacementRules<'_, '_> {
    fn placement_area(&self) -> GridArea {
        **self.placement_session
    }

    /// Requires unreserved cells that are placeable ground or already occupied by the base.
    /// This allows a move to overlap the base's current area.
    fn feedback(&self) -> PlacementFeedback {
        let (_, current_area) = *self.main_base;
        PlacementFeedback::every_cell(self.placement_area(), &self.reserved_coords, |coords| {
            current_area.contains(coords) || self.obstacle_grid.get(coords).is_some_and(ObstacleCell::is_placeable_ground)
        })
    }

    fn update_feedback(
        mut rules: MainBasePlacementRules,
        placement_session: Single<(Entity, &PlacementObject), With<PlacementSession>>,
    ) {
        let (placement_session, object) = *placement_session;
        if object.0 != MapObject::Building(BuildingType::MainBase) { return; }
        let feedback = rules.feedback();
        rules.commands.entity(placement_session).insert_if_neq(feedback);
    }

    #[log_tags(Tag::Build)]
    fn on_place_request_do_so(
        trigger: On<PlaceRequest>,
        mut rules: MainBasePlacementRules,
    ) {
        let PlaceRequest(MapObject::Building(BuildingType::MainBase)) = *trigger else { return };
        let (main_base, &area) = *rules.main_base;
        let new_area = rules.placement_area();
        #[info_player("Main Base cannot be moved to {}", new_area.origin)]
        if rules.feedback().validity == PlacementValidity::Invalid { return; }
        #[info_player("Main Base moved to {}", new_area.origin)]
        rules.obstacle_grid.relocate(area, new_area);
        rules.reserved_coords.reserve(new_area);
        rules.commands.entity(main_base).insert(new_area);
    }
}

fn collect_main_bases(
    main_base: Single<(Entity, &GridArea, &IntegrityPoints), With<MainBase>>,
    mut save: SaveWriter,
) {
    let (entity, &area, integrity_points) = main_base.into_inner();
    let id = entity.index_u32();
    let integrity_points = integrity_points.current;
    save.submit(move |ctx| {
        ctx.save_marker("main_bases", id)?;
        ctx.save_grid_position(id, area.origin)?;
        ctx.save_integrity_points(id, integrity_points)?;
        Ok(())
    });
}

fn load_main_bases(ctx: &mut LoadContext) -> LoadResult {
    ctx.for_each_entity("SELECT id FROM main_bases", |ctx, old_id, entity, _| {
        let grid_position = ctx.grid_position(old_id)?;
        let integrity_points = ctx.integrity_points(old_id)?;
        let builder = BuilderMainBase::new(grid_position)
            .with_integrity_points(integrity_points);
        ctx.insert(entity, builder);
        Ok(())
    })
}
