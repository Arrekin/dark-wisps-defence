use bevy::prelude::*;

use alteration::modifiers::prelude::*;
use buildings::prelude::*;
use game_core::prelude::*;
use grids::{obstacles::GridStructureType, prelude::*, search::pathfinding::path_find_energy_beckon};
use resources::prelude::*;
use session::StatsWispsKilled;
use visuals::prelude::*;
use wisps::prelude::*;

use super::materials::{WispLocomotiveMaterial, WispWaterMaterial};

/// Drives each water wisp's material from its [`Locomotion`], turning measured
/// speed into `vigor` (the shader turns that into deform + cadence) and feeding
/// its travel direction.
///
/// To stay cheap for a swarm, it touches the material — and thus triggers a GPU
/// upload — only when vigor or heading actually changes. A wisp cruising in a
/// straight line or sitting idle animates entirely from the shader clock with no
/// upload at all; writes happen only on turns and speed changes.
///
/// Phases use the anchor scheme on `WispWaterMaterial`: on a change we advance each
/// phase by the time it ran under the OLD vigor's rate, then restart the clock at
/// `now`, keeping the shader's extrapolated phase continuous. Rates are `f(vigor)`,
/// so rather than shuttle them through the material we recompute them both here (for
/// the re-anchor) and in the shader (for the extrapolation) — see the rate constants.
pub(crate) fn drive_water_material(
    mut materials: ResMut<Assets<WispWaterMaterial>>,
    time: Res<Time>,
    wisps: Query<(&Locomotion, &MeshMaterial2d<WispWaterMaterial>)>,
) {
    // An input must drift past this (per component) before we re-upload the material.
    const DRIVE_EPSILON: f32 = 0.01;
    // Measured speed (world units/sec) that maps to vigor 1.0; vigor is unbounded above.
    const VIGOR_SWEET_SPOT: f32 = 60.0;
    // Oscillator cadences, radians/sec: rate = rest + swing * vigor. These MUST match
    // the same-named constants in assets/shaders/wisps/water.wgsl, which recomputes the
    // rates from vigor for its phase extrapolation; here they give the OLD rate for the
    // re-anchor. (A divergence between the two shows up as a phase snap on speed changes.)
    const STROKE_RATE_REST: f32 = 3.5;
    const STROKE_RATE_SWING: f32 = 3.5;
    const SURF_RATE_REST: f32 = 1.5;
    const SURF_RATE_SWING: f32 = 6.0;

    // The wrapped clock the shader reads as `globals.time`, so anchors line up.
    let now = time.elapsed_wrapped().as_secs_f32();
    let wrap_period = time.wrap_period().as_secs_f32();

    for (locomotion, material_handle) in wisps.iter() {
        let handle = &material_handle.0;
        let Some(material) = materials.get(handle) else { continue; };

        let velocity = locomotion.velocity();
        let vigor = velocity.length() / VIGOR_SWEET_SPOT; // unbounded; 1.0 at the sweet spot
        // World heading → quad-local sample space (Rectangle UV V axis points down).
        let heading = velocity.normalize_or_zero();
        let heading_x = heading.x;
        let heading_y = -heading.y;

        // Copy what the material already holds so the immutable borrow ends before
        // we ask for a mutable one.
        let applied_vigor = material.vigor;
        let applied_heading_x = material.heading_x;
        let applied_heading_y = material.heading_y;
        let applied_anchor = material.anchor_time;

        let vigor_changed = (vigor - applied_vigor).abs() > DRIVE_EPSILON;
        let heading_changed = (heading_x - applied_heading_x).abs() > DRIVE_EPSILON
            || (heading_y - applied_heading_y).abs() > DRIVE_EPSILON;
        let clock_wrapped = now < applied_anchor;
        if !(vigor_changed || heading_changed || clock_wrapped) { continue; }

        let Some(mut material) = materials.get_mut(handle) else { continue; };
        // Advance each phase by the time it ran under the OLD vigor's rate, then
        // restart the clock at `now` so the shader's extrapolation stays continuous.
        let elapsed = if now >= material.anchor_time {
            now - material.anchor_time
        } else {
            now + wrap_period - material.anchor_time
        };
        material.stroke_anchor_phase += elapsed * (STROKE_RATE_REST + STROKE_RATE_SWING * material.vigor);
        material.surf_anchor_phase += elapsed * (SURF_RATE_REST + SURF_RATE_SWING * material.vigor);
        material.anchor_time = now;
        material.vigor = vigor; // the shader derives deform + cadence from this
        material.heading_x = heading_x;
        material.heading_y = heading_y;
    }
}

/// Feeds the anchor-free wisp materials (electric, light, fire) from each wisp's
/// [`Locomotion`]: `vigor` and `heading` drive whatever motion-reactive look the
/// shader defines. Gated to stay cheap for a swarm — a material re-uploads only
/// when measured velocity drifts past a threshold, so a wisp cruising in a straight
/// line or sitting idle uploads nothing and animates off the GPU clock.
pub(crate) fn drive_wisp_locomotion<M: WispLocomotiveMaterial>(
    mut materials: ResMut<Assets<M>>,
    wisps: Query<(&Locomotion, &MeshMaterial2d<M>)>,
) {
    // Measured velocity must drift this many world units/sec before we re-upload.
    const DRIVE_EPSILON: f32 = 0.5;

    for (locomotion, material_handle) in wisps.iter() {
        let handle = &material_handle.0;
        let Some(material) = materials.get(handle) else { continue; };

        if material.locomotion().velocity().abs_diff_eq(locomotion.velocity(), DRIVE_EPSILON) {
            continue;
        }

        let Some(mut material) = materials.get_mut(handle) else { continue; };
        material.set_locomotion(locomotion.clone());
    }
}

pub(crate) fn move_wisps(
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

pub(crate) fn target_wisps(
    emissions_grid: Res<EmissionsGrid>,
    obstacle_grid: Res<ObstacleGrid>,
    mut wisps_query: Query<(&mut WispState, &mut GridPath, &GridCoords), With<Wisp>>,
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

pub(crate) fn remove_dead_wisps(
    mut commands: Commands,
    mut stats_wisps_killed: ResMut<StatsWispsKilled>,
    mut stock: ResMut<Stock>,
    mut wisps_grid: ResMut<WispsGrid>,
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

pub(crate) fn wisp_charge_attack(
    mut commands: Commands,
    mut damage_messages: MessageWriter<DamageMessage>,
    obstacle_grid: Res<ObstacleGrid>,
    time: Res<Time>,
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
pub(crate) fn collide_wisps(
    mut commands: Commands,
    mut damage_messages: MessageWriter<DamageMessage>,
    grid: Res<ObstacleGrid>,
    mut wisps_grid: ResMut<WispsGrid>,
    wisps: Query<(Entity, &WispState, &GridPath, &IntegrityPoints, &Transform, &GridCoords), (With<Wisp>, Without<Building>)>,
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
        wisps_grid.wisp_remove(*coords, wisp_entity);
        commands.entity(wisp_entity).despawn();
        commands.spawn(BuilderWispAttackEffect(transform.translation.xy()));
    }
}