use std::collections::VecDeque;

use bevy::prelude::Entity;

use game_core::prelude::{GridCoords, Property};

use crate::{
    emissions::{EmissionsGrid, EmissionsType, FloodEmissionsDetails, FloodEmissionsEvaluator, FloodEmissionsMode, FloodTowerRangeMode},
    energy_supply::{EnergySupplyGrid, FloodEnergySupplyMode},
    prelude::*,
    tower_ranges::TowerRangesGrid,
};

use super::common::{CARDINAL_DIRECTIONS, VISITED_GRID};

pub fn flood_emissions<'a>(
    emissions_grid: &mut EmissionsGrid,
    obstacles_grid: &ObstacleGrid,
    start_coords: impl IntoIterator<Item = GridCoords>,
    emissions_details: impl IntoIterator<Item = &'a FloodEmissionsDetails> + Copy,
    should_field_be_flooded: fn(&Field) -> bool,
) {
    VISITED_GRID.with_borrow_mut(|visited_grid| {
        visited_grid.resize_and_reset(obstacles_grid.bounds());
        let mut queue = VecDeque::new();
        let max_range = emissions_details.into_iter().map(|details| details.range).max().unwrap();
        start_coords.into_iter().for_each(|coords| {
            queue.push_back((1, coords));
            visited_grid.set_visited(coords);
            for details in emissions_details {
                apply_emissions_details(emissions_grid, coords, details, 1);
            }
        });
        while let Some((distance, coords)) = queue.pop_front() {
            for (delta_x, delta_y) in CARDINAL_DIRECTIONS {
                let new_coords = coords.shifted((delta_x, delta_y));
                if !new_coords.is_in_bounds(obstacles_grid.bounds())
                    || visited_grid.is_visited(new_coords)
                    || !should_field_be_flooded(&obstacles_grid[new_coords])
                {
                    continue;
                }

                visited_grid.set_visited(new_coords);
                let new_distance = distance + 1;
                for details in emissions_details {
                    if new_distance <= details.range {
                        apply_emissions_details(emissions_grid, new_coords, details, new_distance);
                    }
                }
                if new_distance < max_range {
                    queue.push_back((new_distance, new_coords));
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
            start_value * (-1. * decay * distance as f32).exp()
        },
    } * if matches!(details.mode, FloodEmissionsMode::Increase) { 1. } else { -1. };
    match details.emissions_type {
        EmissionsType::Energy => {
            emissions_grid.add_energy(grid_coords, value);
        }
    }
}

/// Given start_coords, flood the supply in every direction in range
pub fn flood_energy_supply(
    energy_supply_grid: &mut EnergySupplyGrid,
    start_coords: impl IntoIterator<Item = GridCoords>,
    mode: FloodEnergySupplyMode,
    range: alteration::modifiers::prelude::EnergySupplyRange,
    supplier: Entity,
) {
    VISITED_GRID.with_borrow_mut(|visited_grid| {
        visited_grid.resize_and_reset(energy_supply_grid.bounds());
        let mut queue = VecDeque::new();
        start_coords.into_iter().for_each(|coords| {
            match mode {
                FloodEnergySupplyMode::Increase => energy_supply_grid.add_supplier(coords, supplier),
                FloodEnergySupplyMode::Decrease => energy_supply_grid.remove_supplier(coords, supplier),
            }
            queue.push_back((0, coords));
            visited_grid.set_visited(coords);
        });
        while let Some((distance, coords)) = queue.pop_front() {
            for (delta_x, delta_y) in CARDINAL_DIRECTIONS {
                let new_coords = coords.shifted((delta_x, delta_y));
                if !new_coords.is_in_bounds(energy_supply_grid.bounds())
                    || visited_grid.is_visited(new_coords)
                {
                    continue;
                }

                visited_grid.set_visited(new_coords);
                match mode {
                    FloodEnergySupplyMode::Increase => energy_supply_grid.add_supplier(new_coords, supplier),
                    FloodEnergySupplyMode::Decrease => energy_supply_grid.remove_supplier(new_coords, supplier),
                }
                let new_distance = distance + 1;
                if new_distance < range.get() as usize {
                    queue.push_back((new_distance, new_coords));
                }
            }
        }
    });
}


/// Start with the list of generators coords and flood over all connected cells with energy supply
pub fn flood_power_coverage(
    energy_supply_grid: &mut EnergySupplyGrid,
    start_coords: impl IntoIterator<Item = GridCoords>,
){
    energy_supply_grid.reset_all_power_indicators();
    VISITED_GRID.with_borrow_mut(|visited_grid| {
        visited_grid.resize_and_reset(energy_supply_grid.bounds());
        let mut queue = VecDeque::new();
        start_coords.into_iter().for_each(|coords| {
            queue.push_back(coords);
            visited_grid.set_visited(coords);
            energy_supply_grid[coords].set_power(true);
        });
        while let Some(coords) = queue.pop_front() {
            for (delta_x, delta_y) in CARDINAL_DIRECTIONS {
                let new_coords = coords.shifted((delta_x, delta_y));
                if !new_coords.is_in_bounds(energy_supply_grid.bounds())
                    || visited_grid.is_visited(new_coords)
                {
                    continue;
                }

                visited_grid.set_visited(new_coords);
                if energy_supply_grid[new_coords].has_supply() {
                    queue.push_back(new_coords);
                    energy_supply_grid[new_coords].set_power(true);
                }
            }
        }
    });
}


pub fn flood_tower_range(
    tower_ranges_grid: &mut TowerRangesGrid,
    start_coords: impl IntoIterator<Item = GridCoords>,
    mode: FloodTowerRangeMode,
    range: usize,
    tower_entity: Entity,
) {
    VISITED_GRID.with_borrow_mut(|visited_grid| {
        visited_grid.resize_and_reset(tower_ranges_grid.bounds());
        let mut queue = VecDeque::new();
        start_coords.into_iter().for_each(|coords| {
            match mode {
                FloodTowerRangeMode::Add => tower_ranges_grid.add_tower(coords, tower_entity),
                FloodTowerRangeMode::Remove => tower_ranges_grid.remove_tower(coords, tower_entity),
            }
            queue.push_back((0, coords));
            visited_grid.set_visited(coords);
        });
        while let Some((distance, coords)) = queue.pop_front() {
            for (delta_x, delta_y) in CARDINAL_DIRECTIONS {
                let new_coords = coords.shifted((delta_x, delta_y));
                if !new_coords.is_in_bounds(tower_ranges_grid.bounds())
                    || visited_grid.is_visited(new_coords)
                {
                    continue;
                }

                visited_grid.set_visited(new_coords);
                match mode {
                    FloodTowerRangeMode::Add => tower_ranges_grid.add_tower(new_coords, tower_entity),
                    FloodTowerRangeMode::Remove => tower_ranges_grid.remove_tower(new_coords, tower_entity),
                }
                let new_distance = distance + 1;
                if new_distance < range {
                    queue.push_back((new_distance, new_coords));
                }
            }
        }
    });
}
