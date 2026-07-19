use bevy::{platform::collections::HashSet, prelude::*};
use std::marker::ConstParamTy;

use alteration::modifiers::prelude::EnergySupplyRange;
use game_core::prelude::{GridCoords, GridImprint, IsPowered};

use crate::{GridVersion, base::BaseGrid};

// ============================================================================
// How energy supply works
// ============================================================================
//
// The energy supply grid records, per cell, which suppliers cover that cell
// (their flooded range). This is range recording, not conduction.
//
// A separate power flood (`flood_power_coverage`) starts from generators and
// walks outward through any cell that `has_supply()`. This determines which
// cells are actually connected to a generator. Cells the flood reaches are
// powered (yellow on overlay); cells with supply but no flood reach are
// unpowered (orange on overlay).
//
// Two suppliers exchange power when their ranges **overlap** — i.e. they
// share at least one cell on the grid. The suppliers' building imprints
// (coords/shape) are irrelevant for connectivity; only their flooded ranges
// matter. This means a relay does not need to be inside another relay's
// range to connect — their ranges just need to touch.
//
// A supplier that is unpowered is, by definition, isolated: if its range
// overlapped any powered area, the power flood would reach it and it would
// be powered. Unpowered = no overlap with any powered area.
//
// Disabled suppliers (player-disabled) are kept in a separate `disabled_suppliers`
// set per cell. They do not contribute to `has_supply()`, so the power flood
// does not walk through them. The overlay reads this set to render their
// ranges as red dashed outlines.
// ============================================================================

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
    pub mode: SupplierChange,
}

/// What happened to a supplier, as carried by `SupplierChangedEvent`.
///
/// Deliberately state-free: whether a placed supplier lands in the active or
/// disabled set is derived from live entity state at apply time (see
/// `apply_supplier_changes` in grids_internal), so the event can never
/// disagree with the entity. This also makes `Place` overwrite-idempotent —
/// duplicate or reordered emissions converge on the entity's actual state.
/// `Remove` must keep working after despawn, hence the event payload carries
/// coords/imprint/range.
#[derive(ConstParamTy, PartialEq, Eq, Copy, Clone, Debug)]
pub enum SupplierChange {
    /// (Re)stamp the supplier's coverage over its range.
    Place,
    /// Remove the supplier's coverage entirely (from both sets).
    Remove,
}

/// Concrete grid operation for `flood_energy_supply`, resolved by the consumer
/// from `SupplierChange` + live entity state.
#[derive(PartialEq, Eq, Copy, Clone, Debug)]
pub enum FloodEnergySupplyMode {
    Place { as_disabled: bool },
    Remove,
}

#[derive(Clone, Debug, Default)]
pub struct EnergySupplyField {
    suppliers: HashSet<Entity>,
    disabled_suppliers: HashSet<Entity>,
    has_power: bool,
}
impl EnergySupplyField {
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

pub type EnergySupplyGrid = BaseGrid<EnergySupplyField, GridVersion>;

impl EnergySupplyGrid {
    /// (Re)stamp the supplier on this cell, in the active or disabled set.
    /// Idempotent: always removes from both sets first, so repeated/stale
    /// placements converge on the requested state.
    pub fn place_supplier(&mut self, coords: GridCoords, supplier: Entity, as_disabled: bool) {
        let field = &mut self[coords];
        field.remove_from_both(supplier);
        if as_disabled {
            field.add_disabled_supplier(supplier);
        } else {
            field.add_supplier(supplier);
        }
        self.version = self.version.wrapping_add(1);
    }
    pub fn remove_supplier_from_both(&mut self, coords: GridCoords, supplier: Entity) {
        self[coords].remove_from_both(supplier);
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
