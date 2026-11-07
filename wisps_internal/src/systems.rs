use bevy::prelude::*;

use alteration::modifiers::prelude::*;
use game_core::{motion::Locomotion, prelude::*};
use grids::prelude::*;
use indexes::{
    emissions::EmissionsGrid,
    prelude::*,
    obstacles::GridStructure,
    search::pathfinding::path_find_energy_beckon,
    wisps::WispsGrid,
};
use resources::prelude::*;
use visuals::prelude::*;
use wisps::prelude::*;

use super::{
    looks::{WispMotionVisual, WispWaterVisual, wisp_motion},
    spawning::DeathLoot,
};

// Charge attack tuning
const CHARGE_SPEED_MULTIPLIER: f32 = 5.0;
const BACKOFF_SPEED_MULTIPLIER: f32 = 0.5;

// Motion look
/// Minimum change in vigor or a heading coordinate that triggers a visual update.
const DRIVE_EPSILON: f32 = 0.01;

/// Updates water-wisp vigor and heading from [`Locomotion`].
/// Writes only when motion inputs exceed `DRIVE_EPSILON` or the shader clock wraps.
///
/// Before changing vigor, advances each oscillator phase at its previous rate and resets the
/// time anchor. This keeps the shader animation continuous across speed changes.
pub(crate) fn drive_wisp_water_look(
    time: Res<Time>,
    mut wisps: Query<(&Locomotion, &mut WispWaterVisual)>,
) {
    // Rates in radians/sec: rest + swing * vigor. Keep these in sync with the constants in
    // assets/shaders/wisps/water_look.wesl to prevent phase jumps when speed changes.
    const STROKE_RATE_REST: f32 = 3.5;
    const STROKE_RATE_SWING: f32 = 3.5;
    const SURF_RATE_REST: f32 = 1.5;
    const SURF_RATE_SWING: f32 = 6.0;

    // The wrapped clock the shader reads as `globals.time`, so anchors line up.
    let now = time.elapsed_wrapped().as_secs_f32();
    let wrap_period = time.wrap_period().as_secs_f32();

    for (locomotion, mut visual) in wisps.iter_mut() {
        let (vigor, heading) = wisp_motion(locomotion);

        // Read through `Deref` first, so an unchanged wisp is not marked changed.
        let applied_vigor = visual.look.vigor;
        let applied_heading = visual.look.heading;
        let applied_anchor = visual.look.anchor_time;

        let vigor_changed = (vigor - applied_vigor).abs() > DRIVE_EPSILON;
        let heading_changed = (heading - applied_heading).abs().max_element() > DRIVE_EPSILON;
        let clock_wrapped = now < applied_anchor;
        if !(vigor_changed || heading_changed || clock_wrapped) { continue; }

        let look = &mut visual.look;
        // Preserve the phase accumulated at the previous rate before changing vigor.
        let elapsed = if now >= look.anchor_time {
            now - look.anchor_time
        } else {
            now + wrap_period - look.anchor_time
        };
        look.stroke_anchor_phase += elapsed * (STROKE_RATE_REST + STROKE_RATE_SWING * look.vigor);
        look.surf_anchor_phase += elapsed * (SURF_RATE_REST + SURF_RATE_SWING * look.vigor);
        look.anchor_time = now;
        look.vigor = vigor;
        look.heading = heading;
    }
}

/// Updates motion inputs for fire, light and electric visuals from [`Locomotion`].
/// Writes only when vigor or a heading coordinate changes by more than `DRIVE_EPSILON`,
/// avoiding redundant component changes while the shaders continue animating.
pub(crate) fn drive_wisp_motion_look<V: WispMotionVisual>(mut wisps: Query<(&Locomotion, &mut V)>) {
    for (locomotion, mut visual) in wisps.iter_mut() {
        let (vigor, heading) = wisp_motion(locomotion);
        let applied = visual.motion_look();
        let is_changed = (vigor - applied.vigor).abs() > DRIVE_EPSILON
            || (heading - applied.heading).abs().max_element() > DRIVE_EPSILON;
        if !is_changed { continue; }

        let look = visual.motion_look_mut();
        (look.vigor, look.heading) = (vigor, heading);
    }
}

pub(crate) fn move_wisps(
    mut commands: Commands,
    time: Res<Time>,
    mut wisps_grid: ResMut<WispsGrid>,
    mut wisps: Query<(Entity, &WispState, &IntegrityPoints, &MovementSpeed, &mut Transform, &mut GridPath, &mut GridArea), With<Wisp>>,
) {
    for (entity, wisp_state, integrity_points, speed, mut transform, mut grid_path, mut area) in wisps.iter_mut() {
        if !matches!(*wisp_state, WispState::MovingToTarget) || integrity_points.is_dead() { continue; }
        let Some(next_target) = grid_path.next_in_path() else { continue; };
        let world_position = transform.translation.xy();
        let destination = next_target.coords.world_center();
        let step = time.delta_secs() * speed.get();
        transform.translation += (destination - world_position).clamp(Vec2::splat(-step), Vec2::splat(step)).extend(0.);
        // If close enough, remove from path.
        if transform.translation.xy().distance(destination) < 1. {
            grid_path.remove_first();
        }
        // Update grid area
        let new_coords = GridCoords::from_transform(&transform);
        if new_coords != area.origin.coords {
            let position = area.origin.volume_position();
            // Adopt the path step's altitude when entering its cell.
            let new_position = if new_coords == next_target.coords { next_target } else { new_coords.at(position.level) };
            if new_position.level != position.level {
                commands.entity(entity).insert(DrawLevel::from(new_position.level));
            }
            wisps_grid.wisp_move(position, new_position, entity);
            area.origin = new_position.into();
        }
    }
}

pub(crate) fn target_wisps(
    emissions_grid: Res<EmissionsGrid>,
    obstacle_grid: Res<ObstacleGrid>,
    mut wisps: Query<(&mut WispState, &mut GridPath, &GridArea), With<Wisp>>,
) {
    wisps.par_iter_mut().for_each(|(mut wisp_state, mut grid_path, area)| {
        // Retarget is needed when grid has changed or there is no target yet.
        let need_retarget = match *wisp_state {
            WispState::MovingToTarget => grid_path.grid_version != obstacle_grid.version,
            WispState::NeedTarget | WispState::JustSpawned => true,
            WispState::Stranded(grid_version) => grid_version != obstacle_grid.version,
            WispState::Attacking => false,
        };
        if !need_retarget { return; }

        if path_find_energy_beckon(&obstacle_grid, &emissions_grid, area.origin.volume_position(), &mut grid_path.path) {
            *wisp_state = WispState::MovingToTarget;
            grid_path.grid_version = obstacle_grid.version;
        } else {
            *wisp_state = WispState::Stranded(obstacle_grid.version)
        }
    });
}

pub(crate) fn remove_dead_wisps(
    mut commands: Commands,
    mut stock: ResMut<Stock>,
    mut wisps_grid: ResMut<WispsGrid>,
    wisps: Query<(Entity, &IntegrityPoints, &GridArea, &DeathLoot), With<Wisp>>,
) {
    for (wisp_entity, integrity_points, area, death_loot) in wisps.iter() {
        if integrity_points.is_dead() {
            wisps_grid.wisp_remove(area.origin.volume_position(), wisp_entity);
            commands.entity(wisp_entity).despawn();
            commands.trigger(WispDied(wisp_entity));
            for entry in death_loot.0.iter() {
                stock.add(*entry);
            }
        }
    }
}

pub(crate) fn wisp_charge_attack(
    mut commands: Commands,
    mut damage_messages: MessageWriter<DamageMessage>,
    obstacle_grid: Res<ObstacleGrid>,
    time: Res<Time>,
    mut wisps: Query<(&mut WispState, &MovementSpeed, &MaxAttackRange, &GridPath, &mut Transform, &mut WispChargeAttack, &GridArea), With<Wisp>>,
) {
    for (mut wisp_state, speed, attack_range, grid_path, mut transform, mut attack, area) in wisps.iter_mut() {
        let attack_reach = attack_range.get() as usize;
        // First check if moving wisps should switch to attack mode
        if matches!(*wisp_state, WispState::MovingToTarget) {
            // Start attacking with one step left, or when a building occupies the step at attack reach.
            let is_target_in_range = grid_path.distance() == 1
                || grid_path.at_distance(attack_reach).is_some_and(|position| obstacle_grid[position].is_building());
            if is_target_in_range { *wisp_state = WispState::Attacking; }
        }
        if !matches!(*wisp_state, WispState::Attacking) { continue; }
        // Then confirm the target still exists
        let Some(target_position) = grid_path.at_distance(attack_reach) else { continue; };
        let GridStructure::Building { slot: target_slot, .. } = obstacle_grid[target_position] else {
            // If not, then either find new target if we were already at our intended target, or continue moving if we were stopped by an obstacle
            if grid_path.distance() <= attack_reach {
                *wisp_state = WispState::NeedTarget;
            } else {
                *wisp_state = WispState::MovingToTarget;
            }
            continue;
        };
        let (destination, speed_multiplier) = match *attack {
            // Charge means normal movement, just sped up
            WispChargeAttack::Charge => (target_position.coords.world_center(), CHARGE_SPEED_MULTIPLIER),
            // Backoff means to go back half the normal speed to repeat the charge
            WispChargeAttack::Backoff => (area.world_center(), BACKOFF_SPEED_MULTIPLIER),
        };
        let world_position = transform.translation.xy();
        if world_position.distance(destination) < 1. {
            match *attack {
                WispChargeAttack::Charge => {
                    // Already close enough, trigger attack
                    *attack = WispChargeAttack::Backoff;
                    commands.spawn(BuilderWispAttackEffect { world_position, level: target_position.level });

                    damage_messages.write(DamageMessage {
                        target: obstacle_grid.entry(target_slot).entity,
                        amount: 1.,
                    });
                }
                // Already close enough, start charging again
                WispChargeAttack::Backoff => *attack = WispChargeAttack::Charge,
            }
            continue;
        }
        let step = time.delta_secs() * speed.get() * speed_multiplier;
        transform.translation = world_position.move_towards(destination, step).extend(transform.translation.z);
    }
}

/// Fallback damage for wisps that finish their path while still in MovingToTarget (no ranged attack swapped them
/// to Attacking before arrival). Stop using this for attack types that require contact to trigger (i.e. melee-first
/// attacks that should start in MovingToTarget) — at that point a proper Attacking-state transition needs to be
/// defined per wisp type.
pub(crate) fn collide_wisps(
    mut commands: Commands,
    mut damage_messages: MessageWriter<DamageMessage>,
    obstacle_grid: Res<ObstacleGrid>,
    mut wisps_grid: ResMut<WispsGrid>,
    wisps: Query<(Entity, &WispState, &GridPath, &IntegrityPoints, &Transform, &GridArea), With<Wisp>>,
) {
    for (wisp_entity, wisp_state, grid_path, integrity_points, transform, area) in wisps.iter() {
        if !matches!(wisp_state, WispState::MovingToTarget) || integrity_points.is_dead() { continue; }
        if !grid_path.is_empty() { continue; }
        let position = area.origin.volume_position();
        let building_entity = match obstacle_grid[position] {
            GridStructure::Building { slot, .. } => obstacle_grid.entry(slot).entity,
            _ => panic!("Expected a building"),
        };
        damage_messages.write(DamageMessage {
            target: building_entity,
            amount: 1.,
        });
        wisps_grid.wisp_remove(position, wisp_entity);
        commands.entity(wisp_entity).despawn();
        commands.spawn(BuilderWispAttackEffect { world_position: transform.translation.xy(), level: position.level });
    }
}
