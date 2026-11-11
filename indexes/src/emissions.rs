use std::ops::Deref;

use bevy::prelude::*;

use game_core::prelude::{Bounds, GridArea, GridVolumePosition};
use grids::{GridVersion, base::{BaseGrid, GridStack}};

#[derive(Component)]
pub struct EmitterEnergy(pub FloodEmissionsDetails);

/// Marker for `RequestFlag<EmissionsRecalculation>`, which requests an emissions grid rebuild.
/// Set the flag after terrain changes, such as excavation, that have no
/// `EmissionsGridSpreadAffector` to report them.
pub struct EmissionsRecalculation;

#[derive(Message, Debug)]
pub struct EmitterChangedMessage {
    pub emitter_entity: Entity,
    pub area: GridArea,
    pub emissions_details: FloodEmissionsDetails,
}

#[derive(Copy, Clone, Debug, PartialEq)]
pub enum EmissionsType {
    Energy,
}

#[derive(Clone, Default, Debug)]
pub struct Emissions {
    pub energy: f32,
}
#[derive(Default)]
pub struct EmissionsGridVersion {
    pub energy: GridVersion,
}

#[derive(Resource, Default)]
pub struct EmissionsGrid {
    cells: BaseGrid<GridStack<Emissions>, EmissionsGridVersion>,
}
impl Deref for EmissionsGrid {
    type Target = BaseGrid<GridStack<Emissions>, EmissionsGridVersion>;
    fn deref(&self) -> &Self::Target { &self.cells }
}

impl EmissionsGrid {
    pub fn new_with_size(bounds: impl Into<Bounds>) -> Self {
        Self { cells: BaseGrid::new_with_size(bounds) }
    }
    pub fn add_energy(&mut self, position: GridVolumePosition, energy: f32) {
        let emissions = &mut self.cells[position];
        emissions.energy += energy;
        if emissions.energy.abs() < 0.0001 { emissions.energy = 0.; }
        self.cells.version.energy = self.cells.version.energy.wrapping_add(1);
    }
    pub fn reset_energy_emissions(&mut self) {
        self.cells.grid.iter_mut().flatten().for_each(|emissions| {
            emissions.energy = 0.;
        });
        self.cells.version.energy = self.cells.version.energy.wrapping_add(1);
    }
}

/// Emission intensity as a function of distance.
#[derive(Clone, Debug)]
pub enum FloodEmissionsEvaluator {
    /// The same value at every distance.
    Constant(f32),
    /// `growth × distance`.
    Linear{growth: f32},
    /// `start_value × e^(−decay × distance)`.
    ExponentialDecay{start_value: f32, decay: f32},
}

/// Emission type, spread distance, and intensity function used by the flood.
#[derive(Clone, Debug)]
pub struct FloodEmissionsDetails {
    pub emissions_type: EmissionsType,
    pub range: usize,
    pub evaluator: FloodEmissionsEvaluator,
    pub mode: FloodEmissionsMode,
}
impl FloodEmissionsDetails {
    pub fn cloned_with_reversed_mode(&self) -> Self {
        let mut clone = self.clone();
        clone.mode = match self.mode {
            FloodEmissionsMode::Increase => FloodEmissionsMode::Decrease,
            FloodEmissionsMode::Decrease => FloodEmissionsMode::Increase,
        };
        clone
    }
}

#[derive(Copy, Clone, Debug)]
pub enum FloodEmissionsMode {
    Increase,
    Decrease,
}
