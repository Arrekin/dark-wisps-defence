use bevy::prelude::*;

use buildings::prelude::Building;
use game_core::prelude::{BuildingType, DisabledByPlayer, GridCoords, GridImprint, IsOperational, IsPowered, MapObject, TechnicalStateChanged};
use grids::{
    placement::{validator_all_empty, GridsCollectionParam, PlaceRequest, PlacementEmitter, PlacementValidity},
};

pub(crate) fn building_place_emitter() -> Box<dyn PlacementEmitter> {
    Box::new(PlaceRequest::<Building>::default())
}

pub(crate) fn building_validator(map_object: MapObject, coords: GridCoords, imprint: GridImprint, map_data: &GridsCollectionParam) -> PlacementValidity {
    let MapObject::Building(building_type) = map_object else {
        return PlacementValidity::Invalid;
    };

    if validator_all_empty(map_object, coords, imprint, map_data) == PlacementValidity::Invalid {
        return PlacementValidity::Invalid;
    }

    let needs_power = !matches!(building_type, BuildingType::MainBase | BuildingType::EnergyRelay);
    if needs_power && !map_data.energy_supply_grid.is_imprint_powered(coords, imprint) {
        return PlacementValidity::ValidUnpowered;
    }

    PlacementValidity::Valid
}

/// Default observer for `TechnicalStateChanged`. Buildings that want default
/// "powered && !disabled → operational" behavior attach this. Buildings with
/// custom needs attach their own observer instead.
pub(crate) fn on_technical_state_changed_recompute_operational(
    trigger: On<TechnicalStateChanged>,
    mut commands: Commands,
    state: Query<(Has<IsPowered>, Has<DisabledByPlayer>)>,
) {
    let Ok((has_power, is_disabled)) = state.get(trigger.entity) else { return; };
    if has_power && !is_disabled {
        commands.entity(trigger.entity).insert(IsOperational);
    } else {
        commands.entity(trigger.entity).remove::<IsOperational>();
    }
}

// Building sub-parts markers
#[derive(Component)]
pub(crate) struct MarkerTowerRotationalTop(pub Entity);


#[derive(Component)]
pub(crate) struct TowerTopRotation {
    pub speed: f32, // in radians per second
    pub current_angle: f32,
}
#[derive(EntityEvent)]
pub(crate) struct BuildingDestroyRequest(pub Entity);
