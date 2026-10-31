use bevy::prelude::Entity;

use alteration::modifiers::prelude::EnergySupplyRange;
use game_core::prelude::{GridCoords, Property};
use grids::search::{SEARCH_SCRATCH, SearchScratch};

use crate::{
    emissions::{EmissionsGrid, EmissionsType, FloodEmissionsDetails, FloodEmissionsEvaluator, FloodEmissionsMode},
    energy_supply::{EnergySupplyGrid, FloodEnergySupplyMode},
    prelude::*,
};

pub fn flood_emissions<'a>(
    emissions_grid: &mut EmissionsGrid,
    obstacles_grid: &ObstacleGrid,
    start_coords: impl IntoIterator<Item = GridCoords>,
    emissions_details: impl IntoIterator<Item = &'a FloodEmissionsDetails> + Copy,
    should_cell_be_flooded: fn(&ObstacleCell) -> bool,
) {
    SEARCH_SCRATCH.with_borrow_mut(|SearchScratch { visited, distance_queue, .. }| {
        visited.resize_and_clear(obstacles_grid.bounds);
        distance_queue.clear();
        let max_range = emissions_details.into_iter().map(|details| details.range).max().unwrap();
        start_coords.into_iter().for_each(|coords| {
            distance_queue.push_back((1, coords));
            visited.set_visited(coords);
            for details in emissions_details {
                apply_emissions_details(emissions_grid, coords, details, 1);
            }
        });
        while let Some((distance, coords)) = distance_queue.pop_front() {
            for new_coords in obstacles_grid.bounds.cardinal_neighbors(coords) {
                if visited.is_visited(new_coords) || !should_cell_be_flooded(&obstacles_grid[new_coords])
                {
                    continue;
                }

                visited.set_visited(new_coords);
                let new_distance = distance + 1;
                for details in emissions_details {
                    if new_distance <= details.range {
                        apply_emissions_details(emissions_grid, new_coords, details, new_distance);
                    }
                }
                if new_distance < max_range {
                    distance_queue.push_back((new_distance, new_coords));
                }
            }
        }
    });
}

fn apply_emissions_details(
    emissions_grid: &mut EmissionsGrid,
    grid_coords: GridCoords,
    details: &FloodEmissionsDetails,
    distance: usize
) {
    let value = match details.evaluator {
        FloodEmissionsEvaluator::Constant(value) => value,
        FloodEmissionsEvaluator::Linear{growth} => {
            growth * distance as f32
        },
        FloodEmissionsEvaluator::ExponentialDecay{start_value, decay} => {
            start_value * (-decay * distance as f32).exp()
        },
    } * if matches!(details.mode, FloodEmissionsMode::Increase) { 1. } else { -1. };
    match details.emissions_type {
        EmissionsType::Energy => {
            emissions_grid.add_energy(grid_coords, value);
        }
    }
}

/// Adds or removes supplier coverage by flooding cardinal neighbors from `start_coords`.
pub fn flood_energy_supply(
    energy_supply_grid: &mut EnergySupplyGrid,
    start_coords: impl IntoIterator<Item = GridCoords>,
    mode: FloodEnergySupplyMode,
    range: EnergySupplyRange,
    supplier: Entity,
) {
    SEARCH_SCRATCH.with_borrow_mut(|SearchScratch { visited, distance_queue, .. }| {
        visited.resize_and_clear(energy_supply_grid.bounds);
        distance_queue.clear();
        start_coords.into_iter().for_each(|coords| {
            match mode {
                FloodEnergySupplyMode::Place { as_disabled } => energy_supply_grid.place_supplier(coords, supplier, as_disabled),
                FloodEnergySupplyMode::Remove => energy_supply_grid.remove_supplier_from_both(coords, supplier),
            }
            distance_queue.push_back((0, coords));
            visited.set_visited(coords);
        });
        while let Some((distance, coords)) = distance_queue.pop_front() {
            for new_coords in energy_supply_grid.bounds.cardinal_neighbors(coords) {
                if visited.is_visited(new_coords) {
                    continue;
                }

                visited.set_visited(new_coords);
                match mode {
                    FloodEnergySupplyMode::Place { as_disabled } => energy_supply_grid.place_supplier(new_coords, supplier, as_disabled),
                    FloodEnergySupplyMode::Remove => energy_supply_grid.remove_supplier_from_both(new_coords, supplier),
                }
                let new_distance = distance + 1;
                if new_distance < range.get() as usize {
                    distance_queue.push_back((new_distance, new_coords));
                }
            }
        }
    });
}

/// Rebuilds power coverage from generator cells, following cardinal neighbors with active supply.
pub fn flood_power_coverage(
    energy_supply_grid: &mut EnergySupplyGrid,
    start_coords: impl IntoIterator<Item = GridCoords>,
){
    energy_supply_grid.reset_all_power_indicators();
    SEARCH_SCRATCH.with_borrow_mut(|SearchScratch { visited, coords_queue, .. }| {
        visited.resize_and_clear(energy_supply_grid.bounds);
        coords_queue.clear();
        start_coords.into_iter().for_each(|coords| {
            coords_queue.push_back(coords);
            visited.set_visited(coords);
            energy_supply_grid[coords].set_power(true);
        });
        while let Some(coords) = coords_queue.pop_front() {
            for new_coords in energy_supply_grid.bounds.cardinal_neighbors(coords) {
                if visited.is_visited(new_coords) {
                    continue;
                }

                visited.set_visited(new_coords);
                if energy_supply_grid[new_coords].has_supply() {
                    coords_queue.push_back(new_coords);
                    energy_supply_grid[new_coords].set_power(true);
                }
            }
        }
    });
}
