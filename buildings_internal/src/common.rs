use bevy::{ecs::system::SystemParam, prelude::*};

use almanach::prelude::Almanach;
use game_core::prelude::*;
use grids::placement::{
    validator_all_empty, GridObjectPlacer, GridsCollectionParam, PlacementValidity,
};
use logging::prelude::*;
use resources::prelude::Stock;

pub(crate) fn building_validator(map_object: MapObject, origin: GridCoords, imprint: GridImprint, map_data: &GridsCollectionParam) -> PlacementValidity {
    let MapObject::Building(building_type) = map_object else {
        return PlacementValidity::Invalid;
    };

    if validator_all_empty(map_object, origin, imprint, map_data) == PlacementValidity::Invalid {
        return PlacementValidity::Invalid;
    }

    let needs_power = !matches!(building_type, BuildingType::MainBase | BuildingType::EnergyRelay);
    if needs_power && !map_data.energy_supply_grid.is_imprint_powered(origin, imprint) {
        return PlacementValidity::ValidUnpowered;
    }

    PlacementValidity::Valid
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
    placer: Single<'w, 's, (&'static GridCoords, &'static GridImprint), With<GridObjectPlacer>>,
}

impl<'w, 's> BuildingPlacementManager<'w, 's> {
    /// Origin the placer currently sits on.
    pub(crate) fn coords(&self) -> GridCoords { *self.placer.0 }

    /// Footprint the placer currently shows. Respects `GridPlacerOverridePropertyRequest::OverrideImprint`.
    pub(crate) fn imprint(&self) -> GridImprint { *self.placer.1 }

    /// Validate the site, charge the building's cost, reserve the cells.
    /// `None` means refused or unaffordable; nothing is charged in that case.
    pub(crate) fn claim(&mut self, building_type: BuildingType) -> Option<GridCoords> {
        let (coords, imprint) = (self.coords(), self.imprint());
        if !self.is_site_valid(building_type, coords, imprint) { return None; }

        let costs = &self.almanach.get_building_info(building_type).cost;
        if !self.stock.try_pay_costs(costs) {
            Log::info().player().tag(Tag::Build).message("Not enough resources");
            return None;
        }

        self.reserve_and_log(building_type, coords, imprint);
        Some(coords)
    }

    /// Validate the site and reserve the cells, charging nothing.
    pub(crate) fn claim_free(&mut self, building_type: BuildingType) -> Option<GridCoords> {
        let (coords, imprint) = (self.coords(), self.imprint());
        if !self.is_site_valid(building_type, coords, imprint) { return None; }

        self.reserve_and_log(building_type, coords, imprint);
        Some(coords)
    }

    fn is_site_valid(&self, building_type: BuildingType, coords: GridCoords, imprint: GridImprint) -> bool {
        let validate = self.almanach.get_building_info(building_type).validate;
        (validate)(MapObject::Building(building_type), coords, imprint, &self.grids) != PlacementValidity::Invalid
    }

    fn reserve_and_log(&mut self, building_type: BuildingType, coords: GridCoords, imprint: GridImprint) {
        self.grids.reserved_coords.reserve(coords, imprint);
        let name = &self.almanach.get_building_info(building_type).name;
        Log::info().player().tag(Tag::Build).message(format!("'{name}' placed at ({}, {})", coords.x, coords.y));
    }
}

/// Default observer for `TechnicalStateChanged`. Buildings that want default
/// "powered && !disabled → operational" behavior attach this. Buildings with
/// custom needs attach their own observer instead.
pub(crate) fn on_technical_state_changed_recompute_operational(
    trigger: On<TechnicalStateChanged>,
    mut commands: Commands,
    state: Query<(Has<IsPowered>, Has<DisabledByPlayer>, Has<IsOperational>)>,
) {
    let Ok((has_power, is_disabled, has_is_operational)) = state.get(trigger.entity) else { return; };
    let should_be_operational = has_power && !is_disabled;
    if should_be_operational != has_is_operational {
        if should_be_operational {
            commands.entity(trigger.entity).insert(IsOperational);
        } else {
            commands.entity(trigger.entity).remove::<IsOperational>();
        }
    }
}

// Building sub-parts markers
#[derive(Component)]
#[require(ZDepth::TOWER_TOP)]
pub(crate) struct MarkerTowerRotationalTop(pub Entity);


#[derive(Component)]
pub(crate) struct TowerTopRotation {
    pub speed: f32, // in radians per second
    pub current_angle: f32,
}
#[derive(EntityEvent)]
pub(crate) struct BuildingDestroyRequest(pub Entity);
