use bevy::prelude::*;

use game_core::prelude::{GridCoords, GridImprint};

use crate::{GridVersion, base::BaseGrid};

/// Companion component to EmitterEnergy. Use it to mark wheter Emitter is functional.
#[derive(Component, Default)]
pub struct EmitterEnergyEnabled;
#[derive(Component)]
pub struct EmitterEnergy(pub FloodEmissionsDetails);

#[derive(Message, Debug)]
pub struct EmitterChangedEvent {
    pub emitter_entity: Entity,
    pub imprint: GridImprint,
    pub grid_coords: GridCoords,
    pub emissions_details: Vec<FloodEmissionsDetails>,
}

#[derive(Resource, Default)]
pub struct EmissionsEnergyRecalculateAll(pub bool);

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

pub type EmissionsGrid = BaseGrid<Emissions, EmissionsGridVersion>;

impl EmissionsGrid {
    pub fn add_energy(&mut self, coords: GridCoords, energy: f32) {
        self[coords].energy += energy;
        if self[coords].energy.abs() < 0.0001 { self[coords].energy = 0.; }
        self.version.energy = self.version.energy.wrapping_add(1);
    }
    pub fn reset_energy_emissions(&mut self) {
        self.grid.iter_mut().for_each(|emissions| {
            emissions.energy = 0.;
        });
        self.version.energy = self.version.energy.wrapping_add(1);
    }
}

/// Defines how to calculate the emissions as a function of distance
/// `Constant` - value is constant regardless of distance
/// `Linear` - value decreasing linearly with distance
#[derive(Clone, Debug)]
pub enum FloodEmissionsEvaluator {
    Constant(f32),
    Linear{growth: f32},
    ExponentialDecay{start_value: f32, decay: f32},
}

/// Describes what type of emissions, and how far to spread it.
/// The evaluator determines how to calculate the emissions value as the flood spreads
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

#[derive(Copy, Clone, Debug)]
pub enum FloodTowerRangeMode {
    Add,
    Remove,
}
