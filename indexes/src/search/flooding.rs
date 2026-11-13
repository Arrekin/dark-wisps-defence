use bevy::prelude::Entity;

use alteration::modifiers::prelude::EnergySupplyRange;
use game_core::prelude::{GridArea, GridCoords, GridVolumePosition, Property};

use crate::{
    emissions::{EmissionsGrid, EmissionsType, FloodEmissionsDetails, FloodEmissionsEvaluator, FloodEmissionsMode},
    energy_supply::{EnergySupplyGrid, FloodEnergySupplyMode},
    prelude::*,
};

use super::{
    scratch::{SEARCH_SCRATCH, SearchScratch},
    stepping::walkable_neighbors,
};

/// Spreads emissions from the area's volume level through reachable cardinal neighbors.
/// Follows [`walkable_neighbors`], including level changes through ramps.
pub fn flood_emissions<'a>(
    emissions_grid: &mut EmissionsGrid,
    obstacles_grid: &ObstacleGrid,
    area: &GridArea,
    emissions_details: impl IntoIterator<Item = &'a FloodEmissionsDetails> + Copy,
) {
    SEARCH_SCRATCH.with_borrow_mut(|SearchScratch { position_visited, position_distance_queue, .. }| {
        position_visited.resize_and_clear(obstacles_grid.bounds);
        position_distance_queue.clear();
        let max_range = emissions_details.into_iter().map(|details| details.range).max().unwrap();
        let start_level = area.origin.level.volume();
        area.iter().for_each(|coords| {
            let position = coords.at(start_level);
            position_distance_queue.push_back((1, position));
            position_visited.set_visited(position);
            for details in emissions_details {
                apply_emissions_details(emissions_grid, position, details, 1);
            }
        });
        while let Some((distance, position)) = position_distance_queue.pop_front() {
            for new_position in walkable_neighbors(obstacles_grid, position) {
                if position_visited.is_visited(new_position) {
                    continue;
                }

                position_visited.set_visited(new_position);
                let new_distance = distance + 1;
                for details in emissions_details {
                    if new_distance <= details.range {
                        apply_emissions_details(emissions_grid, new_position, details, new_distance);
                    }
                }
                if new_distance < max_range {
                    position_distance_queue.push_back((new_distance, new_position));
                }
            }
        }
    });
}

fn apply_emissions_details(
    emissions_grid: &mut EmissionsGrid,
    position: GridVolumePosition,
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
            emissions_grid.add_energy(position, value);
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
    SEARCH_SCRATCH.with_borrow_mut(|SearchScratch { coords_visited, coords_distance_queue, .. }| {
        coords_visited.resize_and_clear(energy_supply_grid.bounds);
        coords_distance_queue.clear();
        start_coords.into_iter().for_each(|coords| {
            energy_supply_grid.apply_flood(coords, supplier, mode);
            coords_distance_queue.push_back((0, coords));
            coords_visited.set_visited(coords);
        });
        while let Some((distance, coords)) = coords_distance_queue.pop_front() {
            for new_coords in energy_supply_grid.bounds.cardinal_neighbors(coords) {
                if coords_visited.is_visited(new_coords) {
                    continue;
                }

                coords_visited.set_visited(new_coords);
                energy_supply_grid.apply_flood(new_coords, supplier, mode);
                let new_distance = distance + 1;
                if new_distance < range.get() as usize {
                    coords_distance_queue.push_back((new_distance, new_coords));
                }
            }
        }
    });
}

/// Tests whether a supplier placed on `area` with `range` would connect to existing power.
///
/// Power spreads between cardinally adjacent cells with supply. The proposed coverage connects
/// if it overlaps a powered cell or shares an edge with one.
pub fn would_supply_reach_power(energy_supply_grid: &EnergySupplyGrid, area: GridArea, range: EnergySupplyRange) -> bool {
    let range = range.get() as usize;
    SEARCH_SCRATCH.with_borrow_mut(|SearchScratch { coords_visited, coords_distance_queue, .. }| {
        coords_visited.resize_and_clear(energy_supply_grid.bounds);
        coords_distance_queue.clear();
        for coords in area.iter_in_bounds(energy_supply_grid.bounds) {
            if energy_supply_grid[coords].has_power() { return true; }
            coords_distance_queue.push_back((0, coords));
            coords_visited.set_visited(coords);
        }
        // Check one step beyond `range` for powered neighbors touching the coverage boundary.
        while let Some((distance, coords)) = coords_distance_queue.pop_front() {
            for new_coords in energy_supply_grid.bounds.cardinal_neighbors(coords) {
                if coords_visited.is_visited(new_coords) {
                    continue;
                }

                coords_visited.set_visited(new_coords);
                if energy_supply_grid[new_coords].has_power() { return true; }
                let new_distance = distance + 1;
                if new_distance <= range {
                    coords_distance_queue.push_back((new_distance, new_coords));
                }
            }
        }
        false
    })
}

/// Rebuilds power coverage from generator cells, following cardinal neighbors with active supply.
pub fn flood_power_coverage(
    energy_supply_grid: &mut EnergySupplyGrid,
    start_coords: impl IntoIterator<Item = GridCoords>,
) {
    energy_supply_grid.reset_all_power_indicators();
    SEARCH_SCRATCH.with_borrow_mut(|SearchScratch { coords_visited, coords_queue, .. }| {
        coords_visited.resize_and_clear(energy_supply_grid.bounds);
        coords_queue.clear();
        start_coords.into_iter().for_each(|coords| {
            coords_queue.push_back(coords);
            coords_visited.set_visited(coords);
            energy_supply_grid[coords].set_power(true);
        });
        while let Some(coords) = coords_queue.pop_front() {
            for new_coords in energy_supply_grid.bounds.cardinal_neighbors(coords) {
                if coords_visited.is_visited(new_coords) {
                    continue;
                }

                coords_visited.set_visited(new_coords);
                if energy_supply_grid[new_coords].has_supply() {
                    coords_queue.push_back(new_coords);
                    energy_supply_grid[new_coords].set_power(true);
                }
            }
        }
    });
}
