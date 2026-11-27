use bevy::{
    ecs::system::SystemParam,
    prelude::*,
};

use almanach::{BuildingInfo, prelude::*};
use alteration::{
    effects::prelude::*,
    modifiers::prelude::*,
};
use buildings::prelude::*;
use game_core::prelude::*;
use grids::placement::{
    GridPlacerChanged, PlaceRequest, PlaceResult, PlacementFeedback, PlacementFeedbackSystems, PlacementObject, PlacementSession, PlacementValidity,
    ReservedCoords, UiPlacementSession,
};
use indexes::{
    emissions::{EmissionsType, EmitterEnergy, FloodEmissionsDetails, FloodEmissionsEvaluator, FloodEmissionsMode},
    energy_supply::{EnergySupplyPreviewOperation, EnergySupplyPreviewOperations, GeneratorEnergy, SupplierEnergy},
    prelude::{ObstacleCell, ObstacleGrid},
};
use logging::prelude::*;
use overlays::energy_supply::{EnergySupplyOverlayConfig, EnergySupplyOverlaySecondaryMode};
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
            .add_observer(MainBasePlacementRules::on_grid_placer_changed_write_supply_preview)
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
    commands.spawn(BuilderMainBase::new(GridArea::centered_on(center.at(AltitudeLevel::OnGround), imprint).origin));
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
            baseline: ModifierContributions::from([
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
                    (building_info.baseline.clone(), BaselineEffect),
                ]],
            ))
            .observe(on_technical_state_changed_recompute_operational);
        commands.trigger(TechnicalStateChanged { entity, kind: TechnicalChange::JustSpawned });
    }
}

/// Validates and relocates the existing [`MainBase`].
#[derive(SystemParam)]
pub(crate) struct MainBasePlacementRules<'w, 's> {
    commands: Commands<'w, 's>,
    obstacle_grid: ResMut<'w, ObstacleGrid>,
    reserved_coords: ResMut<'w, ReservedCoords>,
    main_base: Single<'w, 's, (Entity, &'static GridArea, &'static EnergySupplyRange), With<MainBase>>,
}
impl MainBasePlacementRules<'_, '_> {
    /// Writes [`EnergySupplyPreviewOperations`] on `placement_session` for relocating the base from
    /// `base_area` to `placement_area`.
    fn write_supply_preview(&mut self, placement_session: Entity, base_area: GridArea, placement_area: GridArea) {
        let (main_base, _, &range) = *self.main_base;
        self.commands.entity(placement_session).insert(EnergySupplyPreviewOperations(vec![
            EnergySupplyPreviewOperation::Disable { supplier: main_base, area: base_area, range },
            EnergySupplyPreviewOperation::Add { area: placement_area, range, is_generator: true },
        ]));
    }

    /// Requires unreserved cells that are placeable ground or already occupied by the base.
    /// This allows a move to overlap the base's current area.
    fn feedback(&self, area: GridArea) -> PlacementFeedback {
        let (_, current_area, _) = *self.main_base;
        PlacementFeedback::every_cell(area, &self.reserved_coords, |coords| {
            current_area.contains(coords) || self.obstacle_grid.get(coords).is_some_and(ObstacleCell::is_placeable_ground)
        })
    }

    #[log_tags(Tag::Build)]
    fn place(&mut self, new_area: GridArea) -> Result<(), PlaceResult> {
        let (main_base, &area, _) = *self.main_base;
        #[info_player("Main Base cannot be moved to {}", new_area.origin)]
        if self.feedback(new_area).validity == PlacementValidity::Invalid { return Err(PlaceResult::Invalid); }
        #[info_player("Main Base moved to {}", new_area.origin)]
        self.obstacle_grid.relocate(area, new_area);
        self.reserved_coords.reserve(new_area);
        self.commands.entity(main_base).insert(new_area);
        Ok(())
    }

    fn update_feedback(
        mut rules: MainBasePlacementRules,
        ui_placement_session: Single<(Entity, &PlacementObject, &GridArea), With<UiPlacementSession>>,
    ) {
        let (ui_placement_session_entity, &PlacementObject(MapObject::Building(BuildingType::MainBase)), &area) = *ui_placement_session else { return };
        let feedback = rules.feedback(area);
        rules.commands.entity(ui_placement_session_entity).insert_if_neq(feedback);
    }

    /// Publishes an energy supply preview for relocating [`MainBase`] to [`UiPlacementSession`]'s
    /// area and enables the energy supply overlay highlight.
    fn on_grid_placer_changed_write_supply_preview(
        _trigger: On<GridPlacerChanged>,
        mut overlay_config: ResMut<EnergySupplyOverlayConfig>,
        mut rules: MainBasePlacementRules,
        ui_placement_session: Single<(Entity, &PlacementObject, &GridArea), With<UiPlacementSession>>,
    ) {
        let (ui_placement_session_entity, &PlacementObject(MapObject::Building(BuildingType::MainBase)), &area) = *ui_placement_session else { return };
        let (_, &base_area, _) = *rules.main_base;
        rules.write_supply_preview(ui_placement_session_entity, base_area, area);
        overlay_config.secondary_mode = Some(EnergySupplyOverlaySecondaryMode::HighlightAll);
    }

    /// Relocates [`MainBase`] and, for [`UiPlacementSession`], updates the supply preview's
    /// `Disable` operation to the new base area.
    fn on_place_request_do_so(
        trigger: On<PlaceRequest>,
        mut rules: MainBasePlacementRules,
        placement_sessions: Query<(&PlacementObject, Option<&GridArea>, Has<UiPlacementSession>), With<PlacementSession>>,
    ) {
        let Ok((&PlacementObject(MapObject::Building(BuildingType::MainBase)), area, is_ui_placement_session)) = placement_sessions.get(trigger.placement_session) else { return };
        let outcome = area.ok_or(PlaceResult::InvalidSession).and_then(|&area| {
            rules.place(area)?;
            if is_ui_placement_session {
                rules.write_supply_preview(trigger.placement_session, area, area);
            }
            Ok(())
        });
        trigger.report(&mut rules.commands, outcome);
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
