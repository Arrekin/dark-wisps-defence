use bevy::{platform::collections::HashSet, prelude::*};

use alteration::modifiers::prelude::EnergySupplyRange;
use game_core::prelude::{GridCoords, GridImprint, IsPowered};

use crate::{GridVersion, base::BaseGrid};

/// Can provide energy to nearby buildings. Does not produce energy.
#[derive(Component, Copy, Clone, Debug)]
#[require(EnergySupplyRange)]
pub struct SupplierEnergy;

// Produces energy
#[derive(Component, Copy, Clone, Debug)]
#[require(IsPowered)]
pub struct GeneratorEnergy;

#[derive(Message)]
pub struct SupplierChangedEvent {
    pub supplier: Entity,
    pub imprint: GridImprint,
    pub grid_coords: GridCoords,
    pub range: EnergySupplyRange,
    pub mode: FloodEnergySupplyMode,
}

#[derive(Copy, Clone, Debug)]
pub enum FloodEnergySupplyMode {
    Increase,
    Decrease,
}

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
