use bevy::{ecs::system::SystemParam, prelude::*};

use almanach::prelude::Almanach;
use game_core::{math::angle_difference, prelude::*};
use grids::placement::{BeginPlacing, PlacementFeedback, PlacementObject, PlacementSession, PlacementValidity, ReservedCoords};
use indexes::{energy_supply::EnergySupplyGrid, prelude::{ObstacleCell, ObstacleGrid}};
use logging::prelude::*;
use persistence::{prelude::*, rusqlite};
use resources::prelude::Stock;
use states::AdminMode;

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

/// Starts the placement session of every building type at the building's imprint.
pub(crate) fn on_begin_placing_start_building_session(
    trigger: On<BeginPlacing>,
    mut commands: Commands,
    almanach: Res<Almanach>,
    placement_session: Single<Entity, With<PlacementSession>>,
) {
    let BeginPlacing { object: MapObject::Building(building_type), origin } = *trigger else { return };
    let imprint = almanach.get_building_info(building_type).grid_imprint;
    commands.entity(*placement_session).insert(GridArea::new(origin, imprint));
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
    pub(crate) energy_supply_grid: Res<'w, EnergySupplyGrid>,
    pub(crate) reserved_coords: ResMut<'w, ReservedCoords>,
    stock: ResMut<'w, Stock>,
    admin_mode: Res<'w, State<AdminMode>>,
    placement_session: Single<'w, 's, (&'static GridArea, &'static PlacementObject), With<PlacementSession>>,
}

impl BuildingPlacementRules<'_, '_> {
    pub(crate) fn placement_area(&self) -> GridArea {
        *self.placement_session.0
    }

    fn building_being_placed(&self) -> BuildingType {
        let MapObject::Building(building_type) = self.placement_session.1.0 else {
            panic!("Building placement rules judged a session placing {:?}", self.placement_session.1.0);
        };
        building_type
    }

    /// Checks ground, reservations, power, and affordability for the session's building.
    /// A valid site without power is reported as `ValidUnpowered`.
    pub(crate) fn feedback(&self) -> PlacementFeedback {
        self.with_cost_check(self.with_power_check(self.placeable_ground_feedback()))
    }

    /// Requires unreserved, placeable ground throughout the session's area.
    pub(crate) fn placeable_ground_feedback(&self) -> PlacementFeedback {
        PlacementFeedback::every_cell(self.placement_area(), &self.reserved_coords, |coords| {
            self.obstacle_grid.get(coords).is_some_and(ObstacleCell::is_placeable_ground)
        })
    }

    /// Changes `Valid` to `ValidUnpowered` when the session's area has no power.
    pub(crate) fn with_power_check(&self, feedback: PlacementFeedback) -> PlacementFeedback {
        if feedback.validity == PlacementValidity::Valid && !self.energy_supply_grid.is_area_powered(self.placement_area()) {
            return feedback.with_validity(PlacementValidity::ValidUnpowered);
        }
        feedback
    }

    /// Sets affordability from the building's Almanach cost and current stock.
    /// Admin placement is always affordable.
    pub(crate) fn with_cost_check(&self, feedback: PlacementFeedback) -> PlacementFeedback {
        let cost = &self.almanach.get_building_info(self.building_being_placed()).cost;
        feedback.with_affordability(self.admin_mode.get().is_enabled() || self.stock.has_all(cost))
    }

    /// Pays the building's cost and reserves the session's area for a site judged `validity`.
    ///
    /// Callers must judge the site from current state. Returns `None` without charging or
    /// reserving if the site is `Invalid` or payment fails. Admin mode skips payment.
    #[log_tags(Tag::Build)]
    pub(crate) fn claim(&mut self, validity: PlacementValidity) -> Option<GridPosition> {
        let area = self.placement_area();
        let building_info = self.almanach.get_building_info(self.building_being_placed());

        #[info_player("'{}' cannot be placed at {}", building_info.name, area.origin)]
        if validity == PlacementValidity::Invalid { return None; }
        #[info_player("Not enough resources")]
        if !self.admin_mode.get().is_enabled() && !self.stock.try_remove_all(&building_info.cost) { return None; }

        #[info_player("'{}' placed at {}", building_info.name, area.origin)]
        self.reserved_coords.reserve(area);
        Some(area.origin)
    }

    /// Keeps feedback current for towers, the exploration center and the forge.
    pub(crate) fn update_feedback(
        mut rules: BuildingPlacementRules,
        placement_session: Single<(Entity, &PlacementObject), With<PlacementSession>>,
    ) {
        let (placement_session, object) = *placement_session;
        let MapObject::Building(BuildingType::Tower(_) | BuildingType::ExplorationCenter | BuildingType::Forge) = object.0 else { return; };
        let feedback = rules.feedback();
        rules.commands.entity(placement_session).insert_if_neq(feedback);
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

/// Rotating top sprite of a tower. Displays the tower's `TowerTopRotation`.
#[derive(Component)]
#[relationship(relationship_target = TowerTop)]
#[require(ZDepth::TowerTop, DrawLevel::Elevation)]
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
    fn save_shooting_elapsed(&self, entity_id: u32, elapsed: f32) -> rusqlite::Result<usize>;
    /// Writes the turret angle to the shared `tower_top_rotations` table.
    fn save_top_angle(&self, entity_id: u32, angle: f32) -> rusqlite::Result<usize>;
}
impl SaveTowerExtension for SaveContext<'_> {
    fn save_shooting_elapsed(&self, entity_id: u32, elapsed: f32) -> rusqlite::Result<usize> {
        self.tx.prepare_cached("INSERT OR REPLACE INTO tower_shooting_timers (entity_id, elapsed) VALUES (?1, ?2)")?.execute(rusqlite::params![entity_id, elapsed])
    }
    fn save_top_angle(&self, entity_id: u32, angle: f32) -> rusqlite::Result<usize> {
        self.tx.prepare_cached("INSERT OR REPLACE INTO tower_top_rotations (entity_id, angle) VALUES (?1, ?2)")?.execute(rusqlite::params![entity_id, angle])
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
