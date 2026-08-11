use bevy::prelude::*;

use alteration::modifiers::prelude::EnergySupplyRange;
use game_core::prelude::{DisabledByPlayer, GridCoords, GridImprint, MapInfo};
use grids::{
    energy_supply::{EnergySupplyGrid, FloodEnergySupplyMode, GeneratorEnergy, SupplierChange, SupplierChangedEvent, SupplierEnergy},
    search::flooding::{flood_energy_supply, flood_power_coverage},
    EnergySupplySystems,
};
use states::prelude::MapLoadingStage;

pub struct EnergySupplyPlugin;
impl Plugin for EnergySupplyPlugin {
    fn build(&self, app: &mut App) {
        app
            .insert_resource(EnergySupplyGrid::new_empty())
            .init_resource::<EnergySupplyRecalculatePower>()
            .add_message::<SupplierChangedEvent>()
            .add_systems(OnExit(MapLoadingStage::LoadMapInfo), |mut commands: Commands, map_info: Res<MapInfo>| { commands.insert_resource(EnergySupplyGrid::new_with_size(map_info.grid_width, map_info.grid_height)); })
            .add_systems(PostUpdate, (
                (
                    apply_supplier_changes,
                    recalculate_power_coverage.run_if(resource_changed::<EnergySupplyRecalculatePower>),
                ).chain().in_set(EnergySupplySystems),
            ))
            .add_observer(on_add_supplier_energy_register_supplier)
            ;
    }
}

#[derive(Resource, Default)]
struct EnergySupplyRecalculatePower(bool);

fn on_add_supplier_energy_register_supplier(
    trigger: On<Add, SupplierEnergy>,
    mut commands: Commands,
) {
    let entity = trigger.entity;
    // All state transitions collapse into two state-free messages: coverage
    // changes re-Place, coverage loss Removes. Which set a Place lands in
    // (active vs disabled) is resolved from live entity state at apply time.
    commands.entity(entity)
        .observe(emit_supplier_changed::<Insert, (GridCoords, EnergySupplyRange, SupplierEnergy, DisabledByPlayer), { SupplierChange::Place }>)
        .observe(emit_supplier_changed::<Discard, (GridCoords, EnergySupplyRange, SupplierEnergy), { SupplierChange::Remove }>)
        .observe(emit_supplier_changed::<Remove, DisabledByPlayer, { SupplierChange::Place }>);
}

fn emit_supplier_changed<E, B, const MODE: SupplierChange>(
    trigger: On<E, B>,
    mut supplier_changed_event_writer: MessageWriter<SupplierChangedEvent>,
    suppliers: Query<(&EnergySupplyRange, &GridCoords, &GridImprint), With<SupplierEnergy>>,
) where
    E: EntityEvent,
    B: Bundle,
{
    let entity = trigger.event_target();
    let Ok((energy_supply_range, grid_coords, grid_imprint)) = suppliers.get(entity) else { return; };

    supplier_changed_event_writer.write(SupplierChangedEvent {
        supplier: entity,
        imprint: *grid_imprint,
        grid_coords: *grid_coords,
        range: *energy_supply_range,
        mode: MODE,
    });
}

fn apply_supplier_changes(
    mut energy_supply_grid: ResMut<EnergySupplyGrid>,
    mut need_recalculate_power: ResMut<EnergySupplyRecalculatePower>,
    mut events: MessageReader<SupplierChangedEvent>,
    suppliers: Query<Has<DisabledByPlayer>, With<SupplierEnergy>>,
) {
    for event in events.read() {
        // Resolve the concrete grid operation from live entity state, so a
        // message can never act on state that was stale at emit time.
        let mode = match event.mode {
            SupplierChange::Remove => FloodEnergySupplyMode::Remove,
            SupplierChange::Place => {
                // Supplier no longer alive with SupplierEnergy by apply time
                // (despawned, or component removed): its Discard observer
                // emitted a Remove for the same range — drop this Place so a
                // dead supplier can't be resurrected (e.g. by the
                // Remove<DisabledByPlayer> that fires during despawn).
                let Ok(as_disabled) = suppliers.get(event.supplier) else { continue; };
                FloodEnergySupplyMode::Place { as_disabled }
            }
        };
        flood_energy_supply(
            &mut energy_supply_grid,
            event.imprint.iter(event.grid_coords),
            mode,
            event.range,
            event.supplier,
        );
        need_recalculate_power.0 = true;
    }
}

fn recalculate_power_coverage(
    mut energy_supply_grid: ResMut<EnergySupplyGrid>,
    mut need_recalculate_power: ResMut<EnergySupplyRecalculatePower>,
    generators_energy: Query<&GridCoords, With<GeneratorEnergy>>,
) {
    if !need_recalculate_power.0 { return; }
    need_recalculate_power.0 = false;

    flood_power_coverage(&mut energy_supply_grid, generators_energy.iter().copied());
}
