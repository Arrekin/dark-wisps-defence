//! # Energy supply
//!
//! Each cell records the suppliers whose ranges cover it. A power flood starts at generators
//! and follows cardinally connected cells with active supply. Reached cells have power;
//! cells with supply but no path to a generator remain unpowered.
//!
//! Connectivity depends on the coverage cells, so suppliers can connect without either
//! building lying inside the other's range. Disabled suppliers have separate membership
//! lists and do not extend power coverage.
//!
//! ## Previews
//!
//! [`EnergySupply`] holds the real grid and a preview grid of the same type. Entities propose
//! changes by holding [`EnergySupplyPreviewOperations`]. While any entity holds them, the preview
//! shows the real grid with every holder's operations applied by the same floods that maintain
//! the real grid, so what the preview shows is what the change produces. Operations add
//! hypothetical suppliers, and disable or enable existing ones.

use std::{marker::ConstParamTy, ops::{Deref, DerefMut}};

use bevy::prelude::*;
use smallvec::SmallVec;

use alteration::modifiers::prelude::EnergySupplyRange;
use game_core::prelude::{Bounds, GridArea, IsPowered};
use grids::{GridVersion, base::{BaseGrid, GridSingle}};

use crate::search::flooding::{flood_energy_supply, flood_power_coverage};

/// Can provide energy to nearby buildings. Does not produce energy.
#[derive(Component, Copy, Clone, Debug)]
#[require(EnergySupplyRange)]
pub struct SupplierEnergy;

/// Produces energy while not disabled by the player.
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

/// Suppliers covering one cell. Cells are covered by few suppliers, so typical lists stay inline
/// and copying a grid of cells doesn't allocate.
type SupplierList = SmallVec<[Entity; 4]>;

#[derive(Clone, Debug, Default)]
pub struct EnergySupplyCell {
    suppliers: SupplierList,
    disabled_suppliers: SupplierList,
    has_power: bool,
}
impl EnergySupplyCell {
    pub fn has_supply(&self) -> bool { !self.suppliers.is_empty() }
    pub fn has_disabled_supply(&self) -> bool { !self.disabled_suppliers.is_empty() }
    pub fn has_supplier(&self, supplier: Entity) -> bool { self.suppliers.contains(&supplier) }
    pub fn has_disabled_supplier(&self, supplier: Entity) -> bool { self.disabled_suppliers.contains(&supplier) }
    pub fn has_power(&self) -> bool { self.has_power }
    pub fn set_power(&mut self, power: bool) { self.has_power = power; }
    pub fn suppliers(&self) -> &[Entity] { &self.suppliers }
    pub fn disabled_suppliers(&self) -> &[Entity] { &self.disabled_suppliers }
    /// Applies a supplier's coverage change. Placing first drops the supplier from both lists.
    pub fn apply_flood(&mut self, supplier: Entity, mode: FloodEnergySupplyMode) {
        Self::remove_from(&mut self.suppliers, supplier);
        Self::remove_from(&mut self.disabled_suppliers, supplier);
        match mode {
            FloodEnergySupplyMode::Place { as_disabled: false } => self.suppliers.push(supplier),
            FloodEnergySupplyMode::Place { as_disabled: true } => self.disabled_suppliers.push(supplier),
            FloodEnergySupplyMode::Remove => {}
        }
    }

    fn remove_from(list: &mut SupplierList, supplier: Entity) {
        if let Some(index) = list.iter().position(|listed| *listed == supplier) {
            list.swap_remove(index);
        }
    }
}

#[derive(Default)]
pub struct EnergySupplyGrid {
    cells: BaseGrid<GridSingle<EnergySupplyCell>, GridVersion>,
    /// Generators power is flooded from, with their areas.
    pub generators: Vec<(Entity, GridArea)>,
}
impl Deref for EnergySupplyGrid {
    type Target = BaseGrid<GridSingle<EnergySupplyCell>, GridVersion>;
    fn deref(&self) -> &Self::Target { &self.cells }
}
impl DerefMut for EnergySupplyGrid {
    fn deref_mut(&mut self) -> &mut Self::Target { &mut self.cells }
}

impl EnergySupplyGrid {
    pub fn new_with_size(bounds: impl Into<Bounds>) -> Self {
        Self { cells: BaseGrid::new_with_size(bounds), generators: Vec::new() }
    }
    /// Copies the cells and generators of `source`, reusing this grid's allocations.
    /// The version stays as it was.
    pub fn copy_from(&mut self, source: &Self) {
        self.cells.copy_from(&source.cells);
        self.generators.clone_from(&source.generators);
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

/// Energy supply of the map: the real grid and a preview of proposed changes.
#[derive(Resource, Default)]
pub struct EnergySupply {
    pub grid: EnergySupplyGrid,
    preview: EnergySupplyGrid,
    /// Version of `grid` the preview was built from, or `None` while no preview exists.
    preview_source_version: Option<GridVersion>,
}
impl EnergySupply {
    pub fn new_with_size(bounds: impl Into<Bounds>) -> Self {
        let bounds = bounds.into();
        Self {
            grid: EnergySupplyGrid::new_with_size(bounds),
            preview: EnergySupplyGrid::new_with_size(bounds),
            ..default()
        }
    }
    /// The grid with every proposed operation applied, or `None` while no entity proposes any.
    pub fn preview(&self) -> Option<&EnergySupplyGrid> {
        self.preview_source_version.is_some().then_some(&self.preview)
    }
    /// Run condition: a preview exists and the real grid changed since it was built.
    pub fn is_preview_outdated(energy_supply: Res<Self>) -> bool {
        energy_supply.preview_source_version.is_some_and(|version| version != energy_supply.grid.version)
    }
    /// Rebuilds the preview from the real grid and the operations of every proposing entity.
    /// Without proposals, the preview becomes inactive and nothing is rebuilt.
    pub fn rebuild_preview<'a>(
        &mut self,
        proposals: impl Iterator<Item = (Entity, &'a EnergySupplyPreviewOperations)>,
    ) {
        let mut proposals = proposals.peekable();
        if proposals.peek().is_none() {
            self.preview_source_version = None;
            return;
        }

        let preview = &mut self.preview;
        preview.copy_from(&self.grid);
        for (proposer, operations) in proposals {
            for operation in &operations.0 {
                match *operation {
                    EnergySupplyPreviewOperation::Add { area, range, is_generator } => {
                        flood_energy_supply(preview, area, FloodEnergySupplyMode::Place { as_disabled: false }, range, proposer);
                        if is_generator {
                            preview.generators.push((proposer, area));
                        }
                    }
                    EnergySupplyPreviewOperation::Disable { supplier, area, range } => {
                        flood_energy_supply(preview, area, FloodEnergySupplyMode::Place { as_disabled: true }, range, supplier);
                        preview.generators.retain(|(generator, _)| *generator != supplier);
                    }
                    EnergySupplyPreviewOperation::Enable { supplier, area, range, is_generator } => {
                        flood_energy_supply(preview, area, FloodEnergySupplyMode::Place { as_disabled: false }, range, supplier);
                        if is_generator {
                            preview.generators.push((supplier, area));
                        }
                    }
                }
            }
        }
        flood_power_coverage(preview);
        self.preview_source_version = Some(self.grid.version);
    }
}

/// Changes to the energy supply proposed by the entity that holds them.
///
/// While any entity holds operations, [`EnergySupply::preview`] shows the grid with all of them
/// applied. Removing the component withdraws the entity's operations.
#[derive(Component, Clone)]
#[component(immutable)]
pub struct EnergySupplyPreviewOperations(pub Vec<EnergySupplyPreviewOperation>);

/// One proposed change. Each operation changes only its own supplier's entries, so operations
/// apply in any order.
#[derive(Clone, Copy)]
pub enum EnergySupplyPreviewOperation {
    /// A hypothetical supplier on `area`, listed in the cells under the proposing entity.
    /// With `is_generator`, power also starts from its area.
    Add { area: GridArea, range: EnergySupplyRange, is_generator: bool },
    /// An existing supplier, covering `range` around `area`, turns disabled. A disabled generator
    /// stops generating.
    Disable { supplier: Entity, area: GridArea, range: EnergySupplyRange },
    /// An existing disabled supplier, covering `range` around `area`, turns active. With
    /// `is_generator`, it generates again.
    Enable { supplier: Entity, area: GridArea, range: EnergySupplyRange, is_generator: bool },
}
