use std::ops::{Deref, DerefMut};

use bevy::prelude::*;

use game_core::prelude::{Bounds, GridArea, GridCoords};
use grids::{GridVersion, base::BaseGrid};

#[derive(Component)]
pub struct EmitterEnergy(pub FloodEmissionsDetails);

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
pub struct EmissionsGrid(BaseGrid<Emissions, EmissionsGridVersion>);
impl Deref for EmissionsGrid {
    type Target = BaseGrid<Emissions, EmissionsGridVersion>;
    fn deref(&self) -> &Self::Target { &self.0 }
}
impl DerefMut for EmissionsGrid {
    fn deref_mut(&mut self) -> &mut Self::Target { &mut self.0 }
}

impl EmissionsGrid {
    pub fn new_with_size(bounds: impl Into<Bounds>) -> Self {
        Self(BaseGrid::new_with_size(bounds))
    }
    pub fn add_energy(&mut self, coords: GridCoords, energy: f32) {
        let emissions = &mut self[coords];
        emissions.energy += energy;
        if emissions.energy.abs() < 0.0001 { emissions.energy = 0.; }
        self.version.energy = self.version.energy.wrapping_add(1);
    }
    pub fn reset_energy_emissions(&mut self) {
        self.grid.iter_mut().for_each(|emissions| {
            emissions.energy = 0.;
        });
        self.version.energy = self.version.energy.wrapping_add(1);
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
