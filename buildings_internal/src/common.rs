use bevy::prelude::*;

use game_core::prelude::*;
use grids::{
    placement::{validator_all_empty, GridsCollectionParam, PlacementValidity},
};

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
