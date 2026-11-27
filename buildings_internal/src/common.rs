use bevy::{ecs::system::SystemParam, prelude::*};

use almanach::prelude::Almanach;
use alteration::modifiers::prelude::{AttackRangeType, ModifierType};
use buildings::prelude::Tower;
use game_core::{math::angle_difference, prelude::*};
use grids::{
    base::GridCell,
    placement::{BeginPlacing, GridPlacerChanged, PlaceResult, PlacementFeedback, PlacementObject, PlacementValidity, ReservedCoords, UiPlacementSession},
    prelude::GridPath,
};
use hud::prelude::FocusedMapObject;
use indexes::{energy_supply::EnergySupply, prelude::{ObstacleCell, ObstacleGrid}};
use logging::prelude::*;
use overlays::{
    energy_supply::{EnergySupplyOverlayConfig, EnergySupplyOverlaySecondaryMode},
    towers_range::{TowersRangeOverlayConfig, TowersRangeOverlaySecondaryMode},
};
use persistence::{prelude::*, rusqlite};
use resources::prelude::Stock;
use states::AdminMode;
use wisps::prelude::Wisp;

/// Attaches a building image to a UI node or world entity.
#[log_tags(Tag::Build)]
pub(crate) fn on_object_face_request_draw_building(
    trigger: On<ObjectFaceRequest>,
    mut commands: Commands,
    almanach: Res<Almanach>,
    grid_imprints: Query<&GridImprint>,
) {
    let ObjectFaceRequest { entity, object: MapObject::Building(building_type), surface, alpha } = *trigger else { return };
    let building_info = almanach.get_building_info(building_type);

    match surface {
        FaceSurface::Ui => {
            commands.entity(entity).insert(ImageNode::new(building_info.sprite.clone()).with_color(Color::WHITE.with_alpha(alpha)));
        }
        FaceSurface::World => {
            #[warn_dev("Building face requested in the world for {entity}, which has no GridImprint or no longer exists")]
            let Ok(grid_imprint) = grid_imprints.get(entity) else { return };
            commands.entity(entity).insert(Sprite {
                image: building_info.sprite.clone(),
                custom_size: Some(grid_imprint.world_size()),
                color: Color::WHITE.with_alpha(alpha),
                ..default()
            });
        }
    }
}

/// Initializes [`GridArea`] on a building placement session at `trigger.origin` using the
/// building's Almanach imprint.
pub(crate) fn on_begin_placing_set_up_building_placement_session(
    trigger: On<BeginPlacing>,
    mut commands: Commands,
    almanach: Res<Almanach>,
    placement_objects: Query<&PlacementObject>,
) {
    let Ok(&PlacementObject(MapObject::Building(building_type))) = placement_objects.get(trigger.placement_session) else { return };
    let imprint = almanach.get_building_info(building_type).grid_imprint;
    commands.entity(trigger.placement_session).insert(GridArea::new(trigger.origin, imprint));
}

/// Highlights energy suppliers covering the target area when placing a non-supplier consumer.
pub(crate) fn on_grid_placer_changed_highlight_area_suppliers(
    _trigger: On<GridPlacerChanged>,
    mut overlay_config: ResMut<EnergySupplyOverlayConfig>,
    ui_placement_session: Single<(&PlacementObject, &GridArea), With<UiPlacementSession>>,
) {
    let (&PlacementObject(MapObject::Building(building_type)), &area) = *ui_placement_session else { return };
    if !building_type.is_energy_consumer() || building_type.is_energy_supplier() { return; }
    overlay_config.secondary_mode = Some(EnergySupplyOverlaySecondaryMode::HighlightAreaSuppliers { area });
}

/// Previews the baseline attack range on the tower range overlay when placing a building with an attack range.
pub(crate) fn on_grid_placer_changed_highlight_attack_range(
    _trigger: On<GridPlacerChanged>,
    almanach: Res<Almanach>,
    mut overlay_config: ResMut<TowersRangeOverlayConfig>,
    ui_placement_session: Single<(&PlacementObject, &GridArea), With<UiPlacementSession>>,
) {
    let (&PlacementObject(MapObject::Building(building_type)), &area) = *ui_placement_session else { return };
    let building_info = almanach.get_building_info(building_type);
    let Some(AttackRangeType(range_type)) = building_info.attack_range else { return };
    let range = AreaRange {
        range_type,
        area,
        min: building_info.baseline.get(ModifierType::MinAttackRange).unwrap_or(0.),
        max: building_info.baseline.get(ModifierType::MaxAttackRange).unwrap_or(0.),
    };
    overlay_config.secondary_mode = Some(TowersRangeOverlaySecondaryMode::HighlightRange { range });
}

/// Highlights a focused tower's range on the tower range overlay.
pub(crate) fn on_insert_focused_map_object_highlight_tower_range(
    trigger: On<Insert<FocusedMapObject>>,
    mut overlay_config: ResMut<TowersRangeOverlayConfig>,
    towers: Query<(), With<Tower>>,
) {
    if !towers.contains(trigger.entity) { return; }
    overlay_config.secondary_mode = Some(TowersRangeOverlaySecondaryMode::HighlightTower { tower: trigger.entity });
}

pub(crate) fn on_remove_focused_map_object_clear_tower_range_highlight(
    trigger: On<Remove<FocusedMapObject>>,
    mut overlay_config: ResMut<TowersRangeOverlayConfig>,
    towers: Query<(), With<Tower>>,
) {
    if !towers.contains(trigger.entity) { return; }
    overlay_config.secondary_mode = None;
}

/// Shared ground, power, and cost checks for building placement.
///
/// Valid sites can be claimed by paying the cost and reserving their cells. Missing power is
/// reported in feedback but does not prevent placement.
#[derive(SystemParam)]
pub(crate) struct BuildingPlacementRules<'w, 's> {
    pub(crate) commands: Commands<'w, 's>,
    pub(crate) almanach: Res<'w, Almanach>,
    pub(crate) obstacle_grid: Res<'w, ObstacleGrid>,
    pub(crate) energy_supply: Res<'w, EnergySupply>,
    pub(crate) reserved_coords: ResMut<'w, ReservedCoords>,
    stock: ResMut<'w, Stock>,
    admin_mode: Res<'w, State<AdminMode>>,
}

impl BuildingPlacementRules<'_, '_> {
    /// Checks ground, reservations, power, and affordability for `building_type` at `area`.
    /// A valid site without power is reported as `ValidUnpowered`.
    pub(crate) fn feedback(&self, building_type: BuildingType, area: GridArea) -> PlacementFeedback {
        self.with_cost_check(self.with_power_check(self.placeable_ground_feedback(area), area), building_type)
    }

    /// Requires unreserved, placeable ground throughout `area`.
    pub(crate) fn placeable_ground_feedback(&self, area: GridArea) -> PlacementFeedback {
        PlacementFeedback::every_cell(area, &self.reserved_coords, |coords| {
            self.obstacle_grid.get(coords).is_some_and(ObstacleCell::is_placeable_ground)
        })
    }

    /// Changes `Valid` to `ValidUnpowered` when `area` has no power.
    pub(crate) fn with_power_check(&self, feedback: PlacementFeedback, area: GridArea) -> PlacementFeedback {
        if feedback.validity == PlacementValidity::Valid && !self.energy_supply.grid.is_area_powered(area) {
            return feedback.with_validity(PlacementValidity::ValidUnpowered);
        }
        feedback
    }

    /// Sets affordability from the building's Almanach cost and current stock.
    /// Admin placement is always affordable.
    pub(crate) fn with_cost_check(&self, feedback: PlacementFeedback, building_type: BuildingType) -> PlacementFeedback {
        let cost = &self.almanach.get_building_info(building_type).cost;
        feedback.with_affordability(self.admin_mode.get().is_enabled() || self.stock.has_all(cost))
    }

    /// Validates `validity`, deducts the building's cost (unless in [`AdminMode`]), reserves
    /// `area`, and spawns the builder returned by `make_builder(area.origin)`.
    ///
    /// Returns `Err` without charging or reserving if `validity` is [`PlacementValidity::Invalid`]
    /// or stock is insufficient.
    #[log_tags(Tag::Build)]
    pub(crate) fn claim<B: Bundle>(
        &mut self,
        building_type: BuildingType,
        area: GridArea,
        validity: PlacementValidity,
        make_builder: impl FnOnce(GridPosition) -> B,
    ) -> Result<(), PlaceResult> {
        let building_info = self.almanach.get_building_info(building_type);

        #[info_player("'{}' cannot be placed at {}", building_info.name, area.origin)]
        if validity == PlacementValidity::Invalid { return Err(PlaceResult::Invalid); }
        #[info_player("Not enough resources")]
        if !self.admin_mode.get().is_enabled() && !self.stock.try_remove_all(&building_info.cost) { return Err(PlaceResult::CannotAfford); }

        #[info_player("'{}' placed at {}", building_info.name, area.origin)]
        self.reserved_coords.reserve(area);
        self.commands.spawn(make_builder(area.origin));
        Ok(())
    }

    /// Validates `building_type` at `area` using [`Self::feedback`] and claims the site.
    pub(crate) fn place<B: Bundle>(&mut self, building_type: BuildingType, area: GridArea, make_builder: impl FnOnce(GridPosition) -> B) -> Result<(), PlaceResult> {
        let validity = self.feedback(building_type, area).validity;
        self.claim(building_type, area, validity, make_builder)
    }

    /// Updates [`PlacementFeedback`] for buildings that use the shared placement rules (towers,
    /// exploration center, and forge).
    pub(crate) fn update_feedback(
        mut rules: BuildingPlacementRules,
        ui_placement_session: Single<(Entity, &PlacementObject, &GridArea), With<UiPlacementSession>>,
    ) {
        let (ui_placement_session_entity, &PlacementObject(MapObject::Building(building_type)), &area) = *ui_placement_session else { return };
        let (BuildingType::Tower(_) | BuildingType::ExplorationCenter | BuildingType::Forge) = building_type else { return };
        let feedback = rules.feedback(building_type, area);
        rules.commands.entity(ui_placement_session_entity).insert_if_neq(feedback);
    }
}

/// Default observer for `TechnicalStateChanged`. Buildings that want default
/// "powered && !disabled → operational" behavior attach this. Buildings with
/// custom needs attach their own observer instead.
#[log_tags(Tag::Build)]
pub(crate) fn on_technical_state_changed_recompute_operational(
    trigger: On<TechnicalStateChanged>,
    mut commands: Commands,
    state: Query<(Has<IsPowered>, Has<DisabledByPlayer>, Has<IsOperational>)>,
) {
    let Ok((has_power, is_disabled, has_is_operational)) = state.get(trigger.entity) else { return; };
    let should_be_operational = has_power && !is_disabled;
    if should_be_operational != has_is_operational {
        if should_be_operational {
            #[debug_dev("Building {} became operational", trigger.entity)]
            commands.entity(trigger.entity).insert(IsOperational);
        } else {
            #[debug_dev("Building {} stopped being operational (powered: {has_power}, disabled: {is_disabled})", trigger.entity)]
            commands.entity(trigger.entity).remove::<IsOperational>();
        }
    }
}

/// All activities related to aiming and targeting.
#[derive(SystemSet, Clone, Copy, Eq, PartialEq, Hash, Debug)]
pub(crate) struct TowerAimingSystems;

/// Target-leading helper for lobbed projectiles.
#[derive(SystemParam)]
pub(crate) struct PredictiveAim<'w, 's> {
    obstacle_grid: Res<'w, ObstacleGrid>,
    wisps: Query<'w, 's, (&'static GridPath, &'static GridArea), With<Wisp>>,
}
impl PredictiveAim<'_, '_> {
    /// Returns the cell `wisp` is moving into, or its current cell if it has no path or its next
    /// path step is a building cell (wisps stop adjacent to buildings to attack).
    pub(crate) fn next_cell(&self, wisp: Entity) -> Option<GridCoords> {
        let (grid_path, area) = self.wisps.get(wisp).ok()?;
        let next = grid_path.next_in_path().filter(|next| {
            !self.obstacle_grid.get(next.coords).is_some_and(|cell| cell.at(next.level).is_building())
        });
        Some(next.unwrap_or(area.origin).coords)
    }
}

/// Rotating top sprite of a tower. Displays the tower's `TowerTopRotation`.
#[derive(Component)]
#[relationship(relationship_target = TowerTop)]
#[require(ZDepth::TowerTop, DrawLevel::OnGround)]
pub(crate) struct TowerTopOf(pub Entity);

#[derive(Component)]
#[relationship_target(relationship = TowerTopOf, linked_spawn)]
pub(crate) struct TowerTop(Entity);
impl TowerTop {
    pub fn top(&self) -> Entity {
        self.0
    }
}

#[derive(Component)]
pub(crate) struct TowerTopRotation {
    /// Radians.
    pub current_angle: f32,
}
impl TowerTopRotation {
    /// Signed shortest rotation from the current facing to `direction`, in radians.
    pub fn angle_to(&self, direction: Vec2) -> f32 {
        angle_difference(direction.to_angle(), self.current_angle)
    }
    /// Angle within which the top counts as facing a direction, in radians.
    const FACING_TOLERANCE: f32 = std::f32::consts::PI / 72.;

    /// Whether the top faces `direction` within [`Self::FACING_TOLERANCE`].
    pub fn is_facing(&self, direction: Vec2) -> bool {
        self.angle_to(direction).abs() <= Self::FACING_TOLERANCE
    }
}

#[derive(EntityEvent)]
pub(crate) struct BuildingDestroyRequest(pub Entity);

pub(crate) trait SaveTowerExtension {
    /// Writes how far into its reload the tower is to the shared `tower_shooting_timers` table.
    fn save_shooting_elapsed(&self, entity_id: u32, elapsed: f32) -> rusqlite::Result<()>;
    /// Writes the turret angle to the shared `tower_top_rotations` table.
    fn save_top_angle(&self, entity_id: u32, angle: f32) -> rusqlite::Result<()>;
}
impl SaveTowerExtension for SaveContext<'_> {
    fn save_shooting_elapsed(&self, entity_id: u32, elapsed: f32) -> rusqlite::Result<()> {
        self.tx.prepare_cached("INSERT OR REPLACE INTO tower_shooting_timers (entity_id, elapsed) VALUES (?1, ?2)")?.execute(rusqlite::params![entity_id, elapsed])?;
        Ok(())
    }
    fn save_top_angle(&self, entity_id: u32, angle: f32) -> rusqlite::Result<()> {
        self.tx.prepare_cached("INSERT OR REPLACE INTO tower_top_rotations (entity_id, angle) VALUES (?1, ?2)")?.execute(rusqlite::params![entity_id, angle])?;
        Ok(())
    }
}

pub(crate) trait LoadTowerExtension {
    fn shooting_elapsed(&self, old_id: u32) -> LoadResult<f32>;
    fn top_angle(&self, old_id: u32) -> LoadResult<f32>;
}
impl LoadTowerExtension for LoadContext<'_> {
    fn shooting_elapsed(&self, old_id: u32) -> LoadResult<f32> {
        self.conn.prepare_cached("SELECT elapsed FROM tower_shooting_timers WHERE entity_id = ?1")
            .and_then(|mut statement| statement.query_row([old_id], |row| row.get(0)))
            .map_err(LoadError::table_read("tower_shooting_timers"))
    }
    fn top_angle(&self, old_id: u32) -> LoadResult<f32> {
        self.conn.prepare_cached("SELECT angle FROM tower_top_rotations WHERE entity_id = ?1")
            .and_then(|mut statement| statement.query_row([old_id], |row| row.get(0)))
            .map_err(LoadError::table_read("tower_top_rotations"))
    }
}
