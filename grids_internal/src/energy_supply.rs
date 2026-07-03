use bevy::prelude::*;

use alteration::modifiers::prelude::EnergySupplyRange;
use game_core::prelude::{GridCoords, GridImprint};
use grids::{
    energy_supply::{EnergySupplyGrid, SupplierEnergy},
    prelude::*,
    search::flooding::{flood_energy_supply, flood_power_coverage},
};
use states::prelude::MapLoadingStage;

#[derive(Resource, Default)]
struct EnergySupplyRecalculatePower(bool);

fn on_add_supplier_energy_register_supplier(
    trigger: On<Add, SupplierEnergy>,
    mut commands: Commands,
) {
    let entity = trigger.entity;
    commands.entity(entity)
        .observe(on_insert_supplier_coords_or_range_emit_added_event)
        .observe(on_discard_supplier_coords_or_range_emit_removed_event);
}

// Detect change in coords or range and trigger supply grid update
fn on_insert_supplier_coords_or_range_emit_added_event(
    trigger: On<Insert, (GridCoords, EnergySupplyRange, SupplierEnergy)>,
    mut supplier_changed_event_writer: MessageWriter<SupplierChangedEvent>,
    suppliers: Query<(&EnergySupplyRange, &GridCoords, &GridImprint), With<SupplierEnergy>>,
) {
    let entity = trigger.entity;
    let Ok((energy_supply_range, grid_coords, grid_imprint)) = suppliers.get(entity) else { return; };

    supplier_changed_event_writer.write(SupplierChangedEvent {
        supplier: entity,
        imprint: *grid_imprint,
        grid_coords: *grid_coords,
        range: *energy_supply_range,
        mode: FloodEnergySupplyMode::Increase,
    });
}
// Detect change in coords or range and trigger supply grid update
fn on_discard_supplier_coords_or_range_emit_removed_event(
    trigger: On<Discard, (GridCoords, EnergySupplyRange, SupplierEnergy)>,
    mut supplier_changed_event_writer: MessageWriter<SupplierChangedEvent>,
    suppliers: Query<(&EnergySupplyRange, &GridCoords, &GridImprint), With<SupplierEnergy>>,

) {
    let entity = trigger.entity;
    let Ok((energy_supply_range, grid_coords, grid_imprint)) = suppliers.get(entity) else { return; };

    supplier_changed_event_writer.write(SupplierChangedEvent {
        supplier: entity,
        imprint: *grid_imprint,
        grid_coords: *grid_coords,
        range: *energy_supply_range,
        mode: FloodEnergySupplyMode::Decrease,
    });
}

fn apply_supplier_changes(
    mut energy_supply_grid: ResMut<EnergySupplyGrid>,
    mut need_recalculate_power: ResMut<EnergySupplyRecalculatePower>,
    mut events: MessageReader<SupplierChangedEvent>,
) {
    for event in events.read() {
        flood_energy_supply(
            &mut energy_supply_grid,
            event.imprint.iter(event.grid_coords),
            event.mode,
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

fn on_add_needs_power_init_power_state(
    trigger: On<Add, NeedsPower>,
    mut commands: Commands,
    energy_supply_grid: Res<EnergySupplyGrid>,
    mut power_query: Query<(&GridCoords, &GridImprint, &mut NeedsPower)>,
) {
    let entity = trigger.entity;
    let Ok((grid_coords, grid_imprint, mut uses_power)) = power_query.get_mut(entity) else { return; };

    let has_power = energy_supply_grid.is_imprint_powered(*grid_coords, *grid_imprint);
    uses_power.set(&mut commands, entity, has_power);
    commands.entity(entity).observe(on_insert_needs_power_coords_refresh_power_state);
}

/// Local observer triggered when GridCoords or GridImprint changes on UsesPower entities
fn on_insert_needs_power_coords_refresh_power_state(
    trigger: On<Insert, (GridCoords, GridImprint)>,
    mut commands: Commands,
    energy_supply_grid: Res<EnergySupplyGrid>,
    mut power_query: Query<(&GridCoords, &GridImprint, &mut NeedsPower)>,
) {
    let entity = trigger.entity;
    let Ok((grid_coords, grid_imprint, mut uses_power)) = power_query.get_mut(entity) else { return; };

    let has_power = energy_supply_grid.is_imprint_powered(*grid_coords, *grid_imprint);
    uses_power.set(&mut commands, entity, has_power);
}

/// System that updates power states when the energy grid changes
fn update_power_state(
    mut commands: Commands,
    energy_supply_grid: Res<EnergySupplyGrid>,
    mut power_entities: Query<(Entity, &GridCoords, &GridImprint, &mut NeedsPower)>,
    mut current_energy_supply_grid_version: Local<GridVersion>,
) {
    // Only run when energy grid version changes
    if *current_energy_supply_grid_version == energy_supply_grid.version { return; }
    *current_energy_supply_grid_version = energy_supply_grid.version;

    for (entity, grid_coords, grid_imprint, mut uses_power) in power_entities.iter_mut() {
        let has_power = energy_supply_grid.is_imprint_powered(*grid_coords, *grid_imprint);
        uses_power.set(&mut commands, entity, has_power);
    }
}

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
                    update_power_state,
                ).chain(),
            ))
            .add_observer(on_add_supplier_energy_register_supplier)
            .add_observer(on_add_needs_power_init_power_state)
            ;
    }
}
