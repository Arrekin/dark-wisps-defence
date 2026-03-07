use lib_grid::grids::emissions::EmissionsGrid;
use lib_grid::grids::obstacles::{GridStructureType, ObstacleGrid};
use lib_grid::grids::wisps::WispsGrid;
use lib_grid::search::pathfinding::path_find_energy_beckon;
use lib_inventory::stats::StatsWispsKilled;

use crate::visual_effects::wisp_attack::BuilderWispAttackEffect;
use crate::prelude::*;

use super::components::{Wisp, WispChargeAttack, WispState};

pub fn move_wisps(
    time: Res<Time>,
    mut wisps_grid: ResMut<WispsGrid>,
    mut wisps: Query<(Entity, &WispState, &IntegrityPoints, &MovementSpeed, &mut Transform, &mut GridPath, &mut GridCoords), With<Wisp>>,
) {
    for (entity, wisp_state, integrity_points, speed, mut transform, mut grid_path, mut grid_coords) in wisps.iter_mut() {
        if !matches!(*wisp_state, WispState::MovingToTarget) || integrity_points.is_dead() { continue; }
        let Some(next_target) = grid_path.next_in_path() else { continue; };
        let curr_world_coords = transform.translation.truncate();
        let interim_target_world_coords = next_target.to_world_position_centered(GridImprint::default());
        let direction = interim_target_world_coords - curr_world_coords;
        let (sx, sy) = (direction.x.signum(), direction.y.signum());
        let wisp_speed = speed.get();
        transform.translation += Vec3::new(sx * time.delta_secs() * wisp_speed, sy * time.delta_secs() * wisp_speed, 0.);
        // If close enough, remove from path.
        if (transform.translation.truncate().distance(interim_target_world_coords)) < 1. {
            grid_path.remove_first();
        }
        // Update grid coords
        let new_coords = GridCoords::from_transform(&transform);
        if new_coords != *grid_coords {
            wisps_grid.wisp_move(*grid_coords, new_coords, entity);
            *grid_coords = new_coords;
        }
    }
}

pub fn target_wisps(
    mut wisps_query: Query<(&mut WispState, &mut GridPath, &GridCoords), With<Wisp>>,
    obstacle_grid: Res<ObstacleGrid>,
    emissions_grid: Res<EmissionsGrid>,
) {
    wisps_query.par_iter_mut().for_each(|(mut wisp_state, mut grid_path, grid_coords)| {
        // Retarget is needed when grid has changed or there is no target yet.
        let is_path_outdated = matches!(*wisp_state, WispState::MovingToTarget) && grid_path.grid_version != obstacle_grid.version;
        let need_retarget = is_path_outdated || matches!(*wisp_state, WispState::NeedTarget | WispState::JustSpawned) || matches!(*wisp_state, WispState::Stranded(ref grid_version) if obstacle_grid.version != *grid_version);
        if !need_retarget { return; }

        if let Some(path) = path_find_energy_beckon(&obstacle_grid, &emissions_grid, *grid_coords) {
            *wisp_state = WispState::MovingToTarget;
            grid_path.grid_version = obstacle_grid.version;
            grid_path.path = path.into();
        } else {
            *wisp_state = WispState::Stranded(obstacle_grid.version)
        }
    });
}

pub fn remove_dead_wisps(
    mut commands: Commands,
    mut stock: ResMut<Stock>,
    mut wisps_grid: ResMut<WispsGrid>,
    mut stats_wisps_killed: ResMut<StatsWispsKilled>,
    wisps: Query<(Entity, &IntegrityPoints, &GridCoords, &EssencesContainer), With<Wisp>>,
) {
    for (wisp_entity, integrity_points, coords, essences) in wisps.iter() {
        if integrity_points.is_dead() {
            wisps_grid.wisp_remove(*coords, wisp_entity);
            commands.entity(wisp_entity).despawn();
            // Grant essence
            for essence in essences.0.iter() {
                stock.add(essence.essence_type.into(), essence.amount);
            }
            // Update stats
            stats_wisps_killed.0 += 1;
        }
    }
}

pub fn wisp_charge_attack(
    mut commands: Commands,
    mut damage_messages: MessageWriter<DamageMessage>,
    time: Res<Time>,
    obstacle_grid: Res<ObstacleGrid>,
    mut wisps: Query<(&mut WispState, &MovementSpeed, &AttackRange, &GridPath, &mut Transform, &mut WispChargeAttack, &GridCoords), With<Wisp>>,
) {
    for (mut wisp_state, speed, attack_range, grid_path, mut transform, mut attack, grid_coords) in wisps.iter_mut() {
        // First check if moving wisps should switch to attack mode
        if matches!(*wisp_state, WispState::MovingToTarget) {
            // If wisps is at distance 1 to its target, it's always in range
            if grid_path.distance() == 1 {
                *wisp_state = WispState::Attacking;
            } else if let Some(coords_in_range) = grid_path.at_distance(attack_range.get() as usize) {
                // Otherwise, check if the field in the current range is a building
                if obstacle_grid[coords_in_range].has_building() {
                    *wisp_state = WispState::Attacking;
                }
            }
        }
        if !matches!(*wisp_state, WispState::Attacking) { continue; }
        // Then confirm the target still exists
        let Some(target_coords) = grid_path.at_distance(attack_range.get() as usize) else { continue; };
        let GridStructureType::Building(target_entity, _) = obstacle_grid[target_coords].structure else {
            // If not, then either find new target if we were already at our itended target, or continue moving if we were stopped by an obstacle
            if grid_path.distance() <= attack_range.get() as usize {
                *wisp_state = WispState::NeedTarget;
            } else {
                *wisp_state = WispState::MovingToTarget;
            }
            continue; 
        };
        // --- Charge Attack ---
        // Then execute the attack
        match *attack {
            WispChargeAttack::Charge => {
                // Charge means normal movement, just sped up
                let curr_world_coords = transform.translation.truncate();
                let interim_target_world_coords = target_coords.to_world_position_centered(GridImprint::default());
                let direction = interim_target_world_coords - curr_world_coords;
                let distance = direction.length();
                
                if distance < 1. {
                    // Already close enough, trigger attack
                    *attack = WispChargeAttack::Backoff;
                    commands.spawn(BuilderWispAttackEffect(transform.translation.xy()));

                    // Deal damage to the building
                    damage_messages.write(DamageMessage {
                        target: target_entity,
                        amount: 1.,
                    });
                } else {
                    let wisp_speed = time.delta_secs() * speed.get() * 5.; // Speed up during charge
                    if wisp_speed >= distance {
                        // Would overshoot, just move to target position
                        transform.translation = Vec3::new(interim_target_world_coords.x, interim_target_world_coords.y, transform.translation.z);
                    } else {
                        // Normal movement
                        let normalized_direction = direction / distance;
                        let movement = normalized_direction * wisp_speed;
                        transform.translation += Vec3::new(movement.x, movement.y, 0.);
                    }
                }
            },
            WispChargeAttack::Backoff => {
                // Backoff means to go back half the normal speed to repeat the charge
                let curr_world_coords = transform.translation.truncate();
                let interim_target_world_coords = grid_coords.to_world_position_centered(GridImprint::default());
                let direction = interim_target_world_coords - curr_world_coords;
                let distance = direction.length();
                
                if distance < 1. {
                    // Already close enough, start charging again
                    *attack = WispChargeAttack::Charge;
                } else {
                    let wisp_speed = time.delta_secs() * speed.get() * 0.5;
                    if wisp_speed >= distance {
                        // Would overshoot, just move to target position
                        transform.translation = Vec3::new(interim_target_world_coords.x, interim_target_world_coords.y, transform.translation.z);
                    } else {
                        // Normal movement
                        let normalized_direction = direction / distance;
                        let movement = normalized_direction * wisp_speed;
                        transform.translation += Vec3::new(movement.x, movement.y, 0.);
                    }
                }
            },
        }
    }
}

// Fallback damage for wisps that finish their path while still in MovingToTarget (no ranged attack swapped them
// to Attacking before arrival). Safe to run alongside wisp_charge_attack since that system only acts on
// Attacking-state wisps; the two systems handle disjoint wisp states. Stop using this for attack types that
// require contact to trigger (i.e. melee-first attacks that should start in MovingToTarget) — at that point
// a proper Attacking-state transition needs to be defined per wisp type.
pub fn collide_wisps(
    mut commands: Commands,
    mut damage_messages: MessageWriter<DamageMessage>,
    wisps: Query<(Entity, &WispState, &GridPath, &IntegrityPoints, &Transform, &GridCoords), (With<Wisp>, Without<Building>)>,
    grid: Res<ObstacleGrid>,
    mut wisps_grid: ResMut<WispsGrid>,
) {
    for (wisp_entity, wisp_state, grid_path, integrity_points, transform, coords) in wisps.iter() {
        if !matches!(wisp_state, WispState::MovingToTarget) || integrity_points.is_dead() { continue; }
        if !grid_path.is_empty() { continue; }
        let building_entity = match &grid[*coords].structure {
            GridStructureType::Building(entity, _) => *entity,
            _ => panic!("Expected a building"),
        };
        damage_messages.write(DamageMessage {
            target: building_entity,
            amount: 1.,
        });
        wisps_grid.wisp_remove(*coords, wisp_entity.into());
        commands.entity(wisp_entity).despawn();
        commands.spawn(BuilderWispAttackEffect(transform.translation.xy()));
    }
}