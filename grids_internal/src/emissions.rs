use bevy::prelude::*;

use game_core::prelude::{GridCoords, GridImprint, IsOperational, MapInfo};
use grids::{
    EmissionsGridSpreadAffector,
    emissions::{EmissionsEnergyRecalculateAll, EmissionsGrid, EmitterChangedMessage, EmitterEnergy},
    obstacles::ObstacleGrid,
    search::flooding::flood_emissions,
};
use states::prelude::MapLoadingStage;

pub(crate) struct EmissionsPlugin;
impl Plugin for EmissionsPlugin {
    fn build(&self, app: &mut App) {
        app
            .insert_resource(EmissionsGrid::new_empty())
            .init_resource::<EmissionsEnergyRecalculateAll>()
            .add_message::<EmitterChangedMessage>()
            .add_systems(OnExit(MapLoadingStage::LoadMapInfo), |mut commands: Commands, map_info: Res<MapInfo>| { commands.insert_resource(EmissionsGrid::new_with_size(map_info.grid_bounds)); })
            .add_systems(PostUpdate, update_emissions_grid)
            .add_observer(on_add_emitter_energy_register_emitter)
            .add_observer(on_insert_emissions_spread_affector_flag_for_recalculation)
            .add_observer(on_remove_emissions_spread_affector_flag_for_recalculation);
    }
}

fn on_add_emitter_energy_register_emitter(
    trigger: On<Add, EmitterEnergy>,
    mut commands: Commands,
) {
    let entity = trigger.entity;
    commands.entity(entity)
        .observe(on_insert_emitter_coords_or_operational_emit_added_message)
        .observe(on_discard_emitter_coords_or_operational_emit_removed_message);
}

fn on_insert_emitter_coords_or_operational_emit_added_message(
    trigger: On<Insert, (GridCoords, GridImprint, IsOperational)>,
    mut emitter_changed_messages: MessageWriter<EmitterChangedMessage>,
    emitters: Query<(&GridCoords, &GridImprint, &EmitterEnergy), With<IsOperational>>,
) {
    let entity = trigger.entity;
    let Ok((grid_coords, grid_imprint, emitter)) = emitters.get(entity) else { return; };
    emitter_changed_messages.write(EmitterChangedMessage {
        emitter_entity: entity,
        imprint: *grid_imprint,
        grid_coords: *grid_coords,
        emissions_details: vec![emitter.0.clone()],
    });
}

fn on_discard_emitter_coords_or_operational_emit_removed_message(
    trigger: On<Discard, (GridCoords, GridImprint, IsOperational)>,
    mut emitter_changed_messages: MessageWriter<EmitterChangedMessage>,
    emitters: Query<(&GridCoords, &GridImprint, &EmitterEnergy), With<IsOperational>>,
) {
    let entity = trigger.entity;
    let Ok((grid_coords, grid_imprint, emitter)) = emitters.get(entity) else { return; };
    emitter_changed_messages.write(EmitterChangedMessage {
        emitter_entity: entity,
        imprint: *grid_imprint,
        grid_coords: *grid_coords,
        emissions_details: vec![emitter.0.cloned_with_reversed_mode()],
    });
}

fn on_insert_emissions_spread_affector_flag_for_recalculation(
    _trigger: On<Insert, EmissionsGridSpreadAffector>,
    mut recalculate_all: ResMut<EmissionsEnergyRecalculateAll>,
) {
    recalculate_all.0 = true;
}

fn on_remove_emissions_spread_affector_flag_for_recalculation(
    _trigger: On<Remove, EmissionsGridSpreadAffector>,
    mut recalculate_all: ResMut<EmissionsEnergyRecalculateAll>,
) {
    recalculate_all.0 = true;
}

fn update_emissions_grid(
    mut recalculate_all: ResMut<EmissionsEnergyRecalculateAll>,
    mut emissions_grid: ResMut<EmissionsGrid>,
    mut emitter_changed_messages: MessageReader<EmitterChangedMessage>,
    obstacle_grid: Res<ObstacleGrid>,
    emitters: Query<(&EmitterEnergy, &GridImprint, &GridCoords), With<IsOperational>>,
) {
    if recalculate_all.0 {
        recalculate_all.0 = false;
        emissions_grid.reset_energy_emissions();
        for (emitter, grid_imprint, coords) in emitters.iter() {
            flood_emissions(
                &mut emissions_grid,
                &obstacle_grid,
                grid_imprint.iter(*coords),
                &vec![emitter.0.clone()],
                |field| !field.has_wall(),
            );
        }
        emitter_changed_messages.clear();
    } else {
        for message in emitter_changed_messages.read() {
            flood_emissions(
                &mut emissions_grid,
                &obstacle_grid,
                message.imprint.iter(message.grid_coords),
                &message.emissions_details,
                |field| !field.has_wall(),
            );
        }
    }
}
