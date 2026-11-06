//! # Energy supply
//!
//! Each cell records the suppliers whose ranges cover it. A power flood starts at generators
//! and follows cardinally connected cells with active supply. Reached cells have power;
//! cells with supply but no path to a generator remain unpowered.
//!
//! Connectivity depends on the coverage cells, so suppliers can connect without either
//! building lying inside the other's range. Disabled suppliers have separate membership
//! lists and do not extend power coverage.

use std::{marker::ConstParamTy, ops::{Deref, DerefMut}};

use bevy::{platform::collections::HashSet, prelude::*};

use alteration::modifiers::prelude::EnergySupplyRange;
use game_core::prelude::{Bounds, GridArea, GridCoords, IsPowered};
use grids::{GridVersion, base::{BaseGrid, GridSingle}};

/// Can provide energy to nearby buildings. Does not produce energy.
#[derive(Component, Copy, Clone, Debug)]
#[require(EnergySupplyRange)]
pub struct SupplierEnergy;

/// Produces energy.
#[derive(Component, Copy, Clone, Debug)]
#[require(IsPowered)]
pub struct GeneratorEnergy;

/// Updates a supplier's grid coverage.
///
/// Carries the area and range so removal can be processed after the supplier despawns.
#[derive(Message)]
pub struct SupplierChangedMessage {
    pub supplier: Entity,
    pub area: GridArea,
    pub range: EnergySupplyRange,
    pub mode: SupplierChange,
}

/// The coverage change requested by [`SupplierChangedMessage`].
///
/// Placement uses the supplier's live `DisabledByPlayer` state when the message is applied.
/// It is skipped if the entity no longer has `SupplierEnergy`.
#[derive(ConstParamTy, PartialEq, Eq, Copy, Clone, Debug)]
pub enum SupplierChange {
    /// Add or refresh the supplier's coverage.
    Place,
    /// Remove the supplier from both active and disabled coverage.
    Remove,
}

/// Grid operation for `flood_energy_supply`, with the supplier's disabled state resolved.
#[derive(PartialEq, Eq, Copy, Clone, Debug)]
pub enum FloodEnergySupplyMode {
    Place { as_disabled: bool },
    Remove,
}

#[derive(Clone, Debug, Default)]
pub struct EnergySupplyCell {
    suppliers: HashSet<Entity>,
    disabled_suppliers: HashSet<Entity>,
    has_power: bool,
}
impl EnergySupplyCell {
    pub fn has_supply(&self) -> bool { !self.suppliers.is_empty() }
    pub fn has_disabled_supply(&self) -> bool { !self.disabled_suppliers.is_empty() }
    pub fn add_supplier(&mut self, supplier: Entity) { self.suppliers.insert(supplier); }
    pub fn add_disabled_supplier(&mut self, supplier: Entity) { self.disabled_suppliers.insert(supplier); }
    pub fn remove_from_both(&mut self, supplier: Entity) { self.suppliers.remove(&supplier); self.disabled_suppliers.remove(&supplier); }
    pub fn has_supplier(&self, supplier: Entity) -> bool { self.suppliers.contains(&supplier) }
    pub fn has_disabled_supplier(&self, supplier: Entity) -> bool { self.disabled_suppliers.contains(&supplier) }
    pub fn has_power(&self) -> bool { self.has_power }
    pub fn set_power(&mut self, power: bool) { self.has_power = power; }
    pub fn suppliers(&self) -> &HashSet<Entity> { &self.suppliers }
    pub fn disabled_suppliers(&self) -> &HashSet<Entity> { &self.disabled_suppliers }
}

#[derive(Resource, Default)]
pub struct EnergySupplyGrid(BaseGrid<GridSingle<EnergySupplyCell>, GridVersion>);
impl Deref for EnergySupplyGrid {
    type Target = BaseGrid<GridSingle<EnergySupplyCell>, GridVersion>;
    fn deref(&self) -> &Self::Target { &self.0 }
}
impl DerefMut for EnergySupplyGrid {
    fn deref_mut(&mut self) -> &mut Self::Target { &mut self.0 }
}

impl EnergySupplyGrid {
    pub fn new_with_size(bounds: impl Into<Bounds>) -> Self {
        Self(BaseGrid::new_with_size(bounds))
    }
    /// Records the supplier in the cell's active or disabled set.
    /// Removes prior membership from both sets before applying the requested state.
    pub fn place_supplier(&mut self, coords: GridCoords, supplier: Entity, as_disabled: bool) {
        let cell = &mut self[coords];
        cell.remove_from_both(supplier);
        if as_disabled {
            cell.add_disabled_supplier(supplier);
        } else {
            cell.add_supplier(supplier);
        }
        self.advance_version();
    }
    pub fn remove_supplier_from_both(&mut self, coords: GridCoords, supplier: Entity) {
        self[coords].remove_from_both(supplier);
        self.advance_version();
    }
    /// Applies a supplier's coverage change at `coords`.
    pub fn apply_flood(&mut self, coords: GridCoords, supplier: Entity, mode: FloodEnergySupplyMode) {
        match mode {
            FloodEnergySupplyMode::Place { as_disabled } => self.place_supplier(coords, supplier, as_disabled),
            FloodEnergySupplyMode::Remove => self.remove_supplier_from_both(coords, supplier),
        }
    }
    /// True if at least one in-bounds cell of the area has energy supply.
    pub fn is_area_suppliable(&self, area: GridArea) -> bool {
        self.cells_in(area).any(|cell| cell.has_supply())
    }
    /// True if at least one in-bounds cell of the area has power.
    pub fn is_area_powered(&self, area: GridArea) -> bool {
        self.cells_in(area).any(|cell| cell.has_power())
    }
    pub fn reset_all_power_indicators(&mut self) {
        self.grid.iter_mut().for_each(|cell| cell.set_power(false));
        self.advance_version();
    }
}
