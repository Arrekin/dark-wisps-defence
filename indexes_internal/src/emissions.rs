use bevy::prelude::*;

use game_core::{prelude::{GridArea, IsOperational, MapInfo, VolumeLevel}, request_flag::RequestFlag};
use grids::{EmissionsGridSpreadAffector, GridVersion, base::GridCell};
use indexes::{
    emissions::{EmissionsGrid, EmissionsRecalculation, EmitterChangedMessage, EmitterEnergy},
    obstacles::ObstacleGrid,
    search::flooding::flood_emissions,
};
use states::prelude::MapLoadingStage;
use visuals::shared_gpu::{SharedGpuBinding, SharedGpuWriter};

pub(crate) struct EmissionsPlugin;
impl Plugin for EmissionsPlugin {
    fn build(&self, app: &mut App) {
        app
            .init_resource::<EmissionsGrid>()
            .init_resource::<RequestFlag<EmissionsRecalculation>>()
            .add_message::<EmitterChangedMessage>()
            .add_systems(OnExit(MapLoadingStage::LoadMapInfo), |mut commands: Commands, map_info: Res<MapInfo>| { commands.insert_resource(EmissionsGrid::new_with_size(map_info.grid_bounds)); })
            .add_systems(PostUpdate, (
                recalculate_emissions.run_if(RequestFlag::<EmissionsRecalculation>::is_requested),
                apply_emitter_changes,
                upload_gpu_emissions_energy,
            ).chain())
            .add_observer(on_add_emitter_energy_register_emitter)
            .add_observer(on_insert_emissions_spread_affector_flag_for_recalculation)
            .add_observer(on_remove_emissions_spread_affector_flag_for_recalculation);
    }
}

fn on_add_emitter_energy_register_emitter(
    trigger: On<Add<EmitterEnergy>>,
    mut commands: Commands,
) {
    let entity = trigger.entity;
    commands.entity(entity)
        .observe(on_insert_emitter_area_or_operational_emit_added_message)
        .observe(on_discard_emitter_area_or_operational_emit_removed_message);
}

fn on_insert_emitter_area_or_operational_emit_added_message(
    trigger: On<Insert<(GridArea, IsOperational)>>,
    mut emitter_changed_messages: MessageWriter<EmitterChangedMessage>,
    emitters: Query<(&GridArea, &EmitterEnergy), With<IsOperational>>,
) {
    let entity = trigger.entity;
    let Ok((area, emitter)) = emitters.get(entity) else { return; };
    emitter_changed_messages.write(EmitterChangedMessage {
        emitter_entity: entity,
        area: *area,
        emissions_details: emitter.0.clone(),
    });
}

fn on_discard_emitter_area_or_operational_emit_removed_message(
    trigger: On<Discard<(GridArea, IsOperational)>>,
    mut emitter_changed_messages: MessageWriter<EmitterChangedMessage>,
    emitters: Query<(&GridArea, &EmitterEnergy), With<IsOperational>>,
) {
    let entity = trigger.entity;
    let Ok((area, emitter)) = emitters.get(entity) else { return; };
    emitter_changed_messages.write(EmitterChangedMessage {
        emitter_entity: entity,
        area: *area,
        emissions_details: emitter.0.cloned_with_reversed_mode(),
    });
}

fn on_insert_emissions_spread_affector_flag_for_recalculation(
    _trigger: On<Insert<EmissionsGridSpreadAffector>>,
    mut recalculation_requested: ResMut<RequestFlag<EmissionsRecalculation>>,
) {
    recalculation_requested.request();
}

fn on_remove_emissions_spread_affector_flag_for_recalculation(
    _trigger: On<Remove<EmissionsGridSpreadAffector>>,
    mut recalculation_requested: ResMut<RequestFlag<EmissionsRecalculation>>,
) {
    recalculation_requested.request();
}

/// Rebuilds emissions from operational emitters.
/// Clears pending change messages because the rebuild already includes their effects.
fn recalculate_emissions(
    mut recalculation_requested: ResMut<RequestFlag<EmissionsRecalculation>>,
    mut emissions_grid: ResMut<EmissionsGrid>,
    mut emitter_changed_messages: ResMut<Messages<EmitterChangedMessage>>,
    obstacle_grid: Res<ObstacleGrid>,
    emitters: Query<(&EmitterEnergy, &GridArea), With<IsOperational>>,
) {
    recalculation_requested.clear();
    emissions_grid.reset_energy_emissions();
    for (emitter, area) in emitters.iter() {
        flood_emissions(&mut emissions_grid, &obstacle_grid, area, [&emitter.0]);
    }
    emitter_changed_messages.clear();
}

fn apply_emitter_changes(
    mut emissions_grid: ResMut<EmissionsGrid>,
    mut emitter_changed_messages: MessageReader<EmitterChangedMessage>,
    obstacle_grid: Res<ObstacleGrid>,
) {
    for message in emitter_changed_messages.read() {
        flood_emissions(&mut emissions_grid, &obstacle_grid, &message.area, [&message.emissions_details]);
    }
}

/// Uploads the energy layer when its version changes or a map load replaces the grid.
fn upload_gpu_emissions_energy(
    mut shared_gpu: SharedGpuWriter,
    emissions_grid: Res<EmissionsGrid>,
    mut uploaded_version: Local<Option<GridVersion>>,
) {
    if !emissions_grid.is_added() && *uploaded_version == Some(emissions_grid.version.energy) { return; }
    *uploaded_version = Some(emissions_grid.version.energy);

    shared_gpu.write(SharedGpuBinding::EmissionsEnergy, emissions_grid.grid.iter().map(|cell| cell.at(VolumeLevel::Elevation).energy));
}
