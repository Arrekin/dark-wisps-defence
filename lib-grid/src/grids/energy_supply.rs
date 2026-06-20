use crate::lib_prelude::*;
use crate::grids::base::BaseGrid;
use crate::search::flooding::{flood_energy_supply, flood_power_coverage, FloodEnergySupplyMode};

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
                    on_supplier_changed_system,
                    on_recalculate_power_system.run_if(resource_changed::<EnergySupplyRecalculatePower>),
                    NeedsPower::update_system,
                ).chain(),
            ))
            .add_observer(SupplierEnergy::on_add)
            .add_observer(NeedsPower::on_add)
            ;
    }
}

/// Can provide energy to nearby buildings. Does not produce energy.
#[derive(Component, Copy, Clone, Debug)]
#[require(EnergySupplyRange)]
pub struct SupplierEnergy;
impl SupplierEnergy {
    fn on_add(
        trigger: On<Add, SupplierEnergy>,
        mut commands: Commands,
    ) {
        let entity = trigger.entity;
        commands.entity(entity)
            .observe(Self::refresh_on_insert)
            .observe(Self::refresh_on_replace);
    }

    // Detect change in coords or range and trigger supply grid update
    fn refresh_on_insert(
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
    fn refresh_on_replace(
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
}

// Produces energy
#[derive(Component, Copy, Clone, Debug)]
#[require(HasPower)]
pub struct GeneratorEnergy;

#[derive(Message)]
pub struct SupplierChangedEvent {
    pub supplier: Entity,
    pub imprint: GridImprint,
    pub grid_coords: GridCoords,
    pub range: EnergySupplyRange,
    pub mode: FloodEnergySupplyMode,
}

#[derive(Resource, Default)]
struct EnergySupplyRecalculatePower(bool);


#[derive(Clone, Debug, Default)]
pub struct EnergySupplyField {
    suppliers: HashSet<Entity>,
    has_power: bool,
}
impl EnergySupplyField {
    // Only checks if there is any supplier, not if it has power
    pub fn has_supply(&self) -> bool { !self.suppliers.is_empty() }
    pub fn add_supplier(&mut self, supplier: Entity) { self.suppliers.insert(supplier); }
    pub fn remove_supplier(&mut self, supplier: Entity) { self.suppliers.remove(&supplier); }
    pub fn has_supplier(&self, supplier: Entity) -> bool { self.suppliers.contains(&supplier) }
    pub fn has_power(&self) -> bool { self.has_power }
    pub fn set_power(&mut self, power: bool) { self.has_power = power; }
    pub fn suppliers(&self) -> &HashSet<Entity> { &self.suppliers }
}

pub type EnergySupplyGrid = BaseGrid<EnergySupplyField, GridVersion>;

impl EnergySupplyGrid {
    pub fn add_supplier(&mut self, coords: GridCoords, supplier: Entity) {
        self[coords].add_supplier(supplier);
        self.version = self.version.wrapping_add(1);
    }
    pub fn remove_supplier(&mut self, coords: GridCoords, supplier: Entity) {
        self[coords].remove_supplier(supplier);
        self.version = self.version.wrapping_add(1);
    }
    /// At least one of the imprint's cells must have energy supply.
    pub fn is_imprint_suppliable(&self, coords: GridCoords, imprint: GridImprint) -> bool {
        imprint.iter_in_bounds(coords, self.bounds()).any(|c| self[c].has_supply())
    }
    /// At least one of the imprint's cells must have power.
    pub fn is_imprint_powered(&self, coords: GridCoords, imprint: GridImprint) -> bool {
        imprint.iter_in_bounds(coords, self.bounds()).any(|c| self[c].has_power())
    }
    pub fn reset_all_power_indicators(&mut self) {
        self.grid.iter_mut().for_each(|field| field.set_power(false));
        self.version = self.version.wrapping_add(1);
    }
}

fn on_supplier_changed_system(
    mut need_recalculate_power: ResMut<EnergySupplyRecalculatePower>,
    mut events: MessageReader<SupplierChangedEvent>,
    mut energy_supply_grid: ResMut<EnergySupplyGrid>,
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

fn on_recalculate_power_system(
    mut energy_supply_grid: ResMut<EnergySupplyGrid>,
    mut need_recalculate_power: ResMut<EnergySupplyRecalculatePower>,
    generators_energy: Query<&GridCoords, With<GeneratorEnergy>>,
) {
    if !need_recalculate_power.0 { return; }
    need_recalculate_power.0 = false;

    flood_power_coverage(&mut energy_supply_grid, generators_energy.iter().copied());
}

// ============================================================================
// Power State Components
// ============================================================================

/// Component indicating that an entity uses power and should have its power state managed.
/// Automatically manages HasPower/NoPower companion components based on energy grid state.
/// Also stores the current power state directly for convenience access.
#[derive(Component, Default)]
#[require(GridCoords, GridImprint, NoPower)]
pub struct NeedsPower {
    pub has_power: bool,
}
#[derive(Component, Default)]
pub struct HasPower;
#[derive(Component, Default)]
pub struct NoPower;

impl NeedsPower {
    fn on_add(
        trigger: On<Add, NeedsPower>,
        mut commands: Commands,
        energy_supply_grid: Res<EnergySupplyGrid>,
        mut power_query: Query<(&GridCoords, &GridImprint, &mut NeedsPower)>,
    ) {
        let entity = trigger.entity;
        let Ok((grid_coords, grid_imprint, mut uses_power)) = power_query.get_mut(entity) else { return; };

        let has_power = energy_supply_grid.is_imprint_powered(*grid_coords, *grid_imprint);
        uses_power.set(&mut commands, entity, has_power);
        commands.entity(entity).observe(Self::on_coords_change);
    }

    /// Local observer triggered when GridCoords or GridImprint changes on UsesPower entities
    fn on_coords_change(
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

    /// Set the expected value and manage companion components
    fn set(&mut self, commands: &mut Commands, entity: Entity, has_power: bool) {
        if self.has_power != has_power {
            if has_power {
                commands.entity(entity).remove::<NoPower>().insert(HasPower);
            } else {
                commands.entity(entity).remove::<HasPower>().insert(NoPower);
            }
        }
        self.has_power = has_power;
    }

    /// System that updates power states when the energy grid changes
    fn update_system(
        energy_supply_grid: Res<EnergySupplyGrid>,
        mut commands: Commands,
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
}
