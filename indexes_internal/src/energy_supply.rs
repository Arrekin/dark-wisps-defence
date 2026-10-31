use bevy::prelude::*;

use alteration::modifiers::prelude::EnergySupplyRange;
use game_core::{prelude::{DisabledByPlayer, GridArea, MapInfo}, request_flag::RequestFlag};
use grids::EnergySupplySystems;
use indexes::{
    energy_supply::{EnergySupplyGrid, FloodEnergySupplyMode, GeneratorEnergy, SupplierChange, SupplierChangedMessage, SupplierEnergy},
    search::flooding::{flood_energy_supply, flood_power_coverage},
};
use states::prelude::MapLoadingStage;

pub(crate) struct EnergySupplyPlugin;
impl Plugin for EnergySupplyPlugin {
    fn build(&self, app: &mut App) {
        app
            .init_resource::<EnergySupplyGrid>()
            .init_resource::<RequestFlag<PowerCoverageRecalculation>>()
            .add_message::<SupplierChangedMessage>()
            .add_systems(OnExit(MapLoadingStage::LoadMapInfo), |mut commands: Commands, map_info: Res<MapInfo>| { commands.insert_resource(EnergySupplyGrid::new_with_size(map_info.grid_bounds)); })
            .add_systems(PostUpdate, (
                (
                    apply_supplier_changes,
                    recalculate_power_coverage.run_if(RequestFlag::<PowerCoverageRecalculation>::is_requested),
                ).chain().in_set(EnergySupplySystems),
            ))
            .add_observer(on_add_supplier_energy_register_supplier);
    }
}

/// Recalculation of power coverage from the generators.
struct PowerCoverageRecalculation;

fn on_add_supplier_energy_register_supplier(
    trigger: On<Add<SupplierEnergy>>,
    mut commands: Commands,
) {
    let entity = trigger.entity;
    // All state transitions collapse into two state-free messages: coverage
    // changes re-Place, coverage loss Removes. Which set a Place lands in
    // (active vs disabled) is resolved from live entity state at apply time.
    commands.entity(entity)
        .observe(emit_supplier_changed::<Insert<(GridArea, EnergySupplyRange, SupplierEnergy, DisabledByPlayer)>, { SupplierChange::Place }>)
        .observe(emit_supplier_changed::<Discard<(GridArea, EnergySupplyRange, SupplierEnergy)>, { SupplierChange::Remove }>)
        .observe(emit_supplier_changed::<Remove<DisabledByPlayer>, { SupplierChange::Place }>);
}

fn emit_supplier_changed<P, const MODE: SupplierChange>(
    trigger: On<P>,
    mut supplier_changed_messages: MessageWriter<SupplierChangedMessage>,
    suppliers: Query<(&EnergySupplyRange, &GridArea), With<SupplierEnergy>>,
) where
    P: EventPattern<Event: EntityEvent>,
{
    let entity = trigger.event_target();
    let Ok((energy_supply_range, area)) = suppliers.get(entity) else { return; };

    supplier_changed_messages.write(SupplierChangedMessage {
        supplier: entity,
        area: *area,
        range: *energy_supply_range,
        mode: MODE,
    });
}

fn apply_supplier_changes(
    mut energy_supply_grid: ResMut<EnergySupplyGrid>,
    mut need_recalculate_power: ResMut<RequestFlag<PowerCoverageRecalculation>>,
    mut supplier_changed_messages: MessageReader<SupplierChangedMessage>,
    suppliers: Query<Has<DisabledByPlayer>, With<SupplierEnergy>>,
) {
    for message in supplier_changed_messages.read() {
        // Resolve the disabled state at apply time; it may have changed since the message was sent.
        let mode = match message.mode {
            SupplierChange::Remove => FloodEnergySupplyMode::Remove,
            SupplierChange::Place => {
                // Ignore placements for removed suppliers. Removing DisabledByPlayer during
                // despawn can enqueue a Place after the Discard observer enqueues a Remove.
                let Ok(as_disabled) = suppliers.get(message.supplier) else { continue; };
                FloodEnergySupplyMode::Place { as_disabled }
            }
        };
        flood_energy_supply(
            &mut energy_supply_grid,
            message.area.iter(),
            mode,
            message.range,
            message.supplier,
        );
        need_recalculate_power.request();
    }
}

fn recalculate_power_coverage(
    mut energy_supply_grid: ResMut<EnergySupplyGrid>,
    mut need_recalculate_power: ResMut<RequestFlag<PowerCoverageRecalculation>>,
    generators: Query<&GridArea, With<GeneratorEnergy>>,
) {
    need_recalculate_power.clear();

    flood_power_coverage(&mut energy_supply_grid, generators.iter().map(|area| area.origin.coords));
}
