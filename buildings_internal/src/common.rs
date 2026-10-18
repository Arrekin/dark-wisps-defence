use bevy::{ecs::system::SystemParam, prelude::*};

use almanach::prelude::Almanach;
use game_core::prelude::*;
use grids::placement::{
    validator_all_empty, GridObjectPlacer, GridsCollectionParam, PlacementValidity,
};
use logging::prelude::*;
use persistence::{prelude::*, rusqlite};
use resources::prelude::Stock;
use states::AdminMode;

pub(crate) fn building_validator(map_object: MapObject, origin: GridCoords, imprint: GridImprint, grids: &GridsCollectionParam) -> PlacementValidity {
    let MapObject::Building(building_type) = map_object else {
        return PlacementValidity::Invalid;
    };

    if validator_all_empty(map_object, origin, imprint, grids) == PlacementValidity::Invalid {
        return PlacementValidity::Invalid;
    }

    let needs_power = !matches!(building_type, BuildingType::MainBase | BuildingType::EnergyRelay);
    if needs_power && !grids.energy_supply_grid.is_imprint_powered(origin, imprint) {
        return PlacementValidity::ValidUnpowered;
    }

    PlacementValidity::Valid
}

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

/// Common placement logic helper for building's `PlaceRequest` observer.
///
/// Two entry points:
/// - [`claim`](Self::claim) — validate the site, pay the building's cost, reserve the cells.
///   Use this for the standard "pay and build" flow.
/// - [`claim_free`](Self::claim_free) — validate and reserve without charging. Use this when
///   the building has its own rules (a cap on how many may exist, a substituted cost, etc.)
///   and handles payment itself, or when nothing is charged at all.
#[derive(SystemParam)]
pub(crate) struct BuildingPlacementManager<'w, 's> {
    almanach: Res<'w, Almanach>,
    grids: GridsCollectionParam<'w>,
    stock: ResMut<'w, Stock>,
    admin_mode: Res<'w, State<AdminMode>>,
    placer: Single<'w, 's, (&'static GridCoords, &'static GridImprint), With<GridObjectPlacer>>,
}

impl<'w, 's> BuildingPlacementManager<'w, 's> {
    /// Origin the placer currently sits on.
    pub(crate) fn coords(&self) -> GridCoords { *self.placer.0 }

    /// Footprint the placer currently shows. Respects `GridPlacerOverridePropertyRequest::OverrideImprint`.
    pub(crate) fn imprint(&self) -> GridImprint { *self.placer.1 }

    /// Validate the site, charge the building's cost, reserve the cells.
    /// `None` means refused or unaffordable; nothing is charged in that case.
    ///
    /// In admin mode placement is free. Validation still runs.
    #[log_tags(Tag::Build)]
    pub(crate) fn claim(&mut self, building_type: BuildingType) -> Option<GridCoords> {
        if self.admin_mode.get().is_enabled() {
            return self.claim_free(building_type);
        }

        let (coords, imprint) = (self.coords(), self.imprint());
        let building_info = self.almanach.get_building_info(building_type);

        #[info_player("'{}' cannot be placed at {coords}", building_info.name)]
        if !self.is_site_valid(building_type, coords, imprint) { return None; }
        #[info_player("Not enough resources")]
        if !self.stock.try_remove_all(&building_info.cost) { return None; }

        #[info_player("'{}' placed at {coords}", building_info.name)]
        self.grids.reserved_coords.reserve(coords, imprint);
        Some(coords)
    }

    /// Validate the site and reserve the cells, charging nothing.
    #[log_tags(Tag::Build)]
    pub(crate) fn claim_free(&mut self, building_type: BuildingType) -> Option<GridCoords> {
        let (coords, imprint) = (self.coords(), self.imprint());
        let building_info = self.almanach.get_building_info(building_type);

        #[info_player("'{}' cannot be placed at {coords}", building_info.name)]
        if !self.is_site_valid(building_type, coords, imprint) { return None; }

        #[info_player("'{}' placed at {coords}", building_info.name)]
        self.grids.reserved_coords.reserve(coords, imprint);
        Some(coords)
    }

    fn is_site_valid(&self, building_type: BuildingType, coords: GridCoords, imprint: GridImprint) -> bool {
        let validate = self.almanach.get_building_info(building_type).validate;
        validate(MapObject::Building(building_type), coords, imprint, &self.grids) != PlacementValidity::Invalid
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

/// Marks a tower's rotating top sprite; holds the tower it belongs to.
#[derive(Component)]
#[require(ZDepth::TowerTop)]
pub(crate) struct MarkerTowerRotationalTop(pub Entity);

#[derive(Component)]
pub(crate) struct TowerTopRotation {
    /// Radians per second.
    pub speed: f32,
    pub current_angle: f32,
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
