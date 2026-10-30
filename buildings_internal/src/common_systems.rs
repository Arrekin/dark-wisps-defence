use std::time::Duration;

use bevy::prelude::*;

use almanach::prelude::*;
use alteration::modifiers::prelude::*;
use buildings::prelude::*;
use game_core::prelude::*;
use grids::{
    prelude::*,
    search::targetfinding::target_find_closest_wisp,
    wisps::WispsGrid,
};
use logging::prelude::*;
use states::prelude::*;
use visuals::prelude::*;
use wisps::prelude::*;

use crate::common::*;

pub(crate) struct CommonSystemsPlugin;
impl Plugin for CommonSystemsPlugin {
    fn build(&self, app: &mut App) {
        app
            .add_systems(PreUpdate, (
                tick_shooting_timers_system,
                damage_control_system,
            ).run_if(in_state(GameState::Running)))
            .add_systems(Update, (
                targeting_system,
                rotational_aiming_system,
            ).chain().in_set(TowerAimingSystems).run_if(in_state(GameState::Running)))
            .add_systems(Update, rotate_tower_top_system.after(TowerAimingSystems))
            .add_observer(on_building_destroy_request_do_so)
            .add_observer(on_insert_attack_speed_sync_shooting_timer)
            .add_observer(on_remove_is_operational_reset_shooting_timer)
            .add_observer(on_object_face_request_draw_building);
    }
}

fn on_insert_attack_speed_sync_shooting_timer(
    trigger: On<Insert<AttackSpeed>>,
    mut timers: Query<(&mut TowerShootingTimer, &AttackSpeed)>,
) {
    let Ok((mut timer, attack_speed)) = timers.get_mut(trigger.entity) else { return; };
    if attack_speed.get() == 0. { return; }
    timer.0.set_duration(Duration::from_secs_f32(1. / attack_speed.get()));
}

/// A tower that stops being operational loses its reload progress.
fn on_remove_is_operational_reset_shooting_timer(
    trigger: On<Remove<IsOperational>>,
    mut timers: Query<&mut TowerShootingTimer>,
) {
    let Ok(mut timer) = timers.get_mut(trigger.entity) else { return; };
    timer.0.reset();
}

fn targeting_system(
    obstacle_grid: Res<ObstacleGrid>,
    wisps_grid: Res<WispsGrid>,
    mut towers: Query<(AttackAreaRange, &mut TowerWispTarget), (With<Tower>, With<IsOperational>)>,
    wisps: Query<&Transform, With<Wisp>>,
) {
    for (attack_range, mut target) in towers.iter_mut() {
        let range = attack_range.area_range();
        let in_range = |wisp: Entity| wisps.get(wisp).is_ok_and(|wisp_transform| range.contains(wisp_transform.translation.xy()));
        match *target {
            TowerWispTarget::Wisp(wisp) if in_range(wisp) => continue,
            TowerWispTarget::NoValidTargets(grid_version) if grid_version == wisps_grid.version => continue,
            TowerWispTarget::Suspended => continue,
            TowerWispTarget::Wisp(_) | TowerWispTarget::NoValidTargets(_) | TowerWispTarget::SearchForNewTarget => {}
        }
        let mut wisp_on_edge = false;
        let found = match range.range_type {
            RangeType::Cells => target_find_closest_wisp(&obstacle_grid, &wisps_grid, &range, true).map(|(_, target_wisp)| target_wisp),
            // Circular attacks use any wisp in range as the firing trigger.
            RangeType::Circle => range.bounding_box().iter_in_bounds(wisps_grid.bounds).find_map(|coords| {
                let cell_wisps = &wisps_grid[coords];
                if cell_wisps.is_empty() { return None; }
                let found = cell_wisps.iter().copied().find(|&wisp| in_range(wisp));
                wisp_on_edge = wisp_on_edge || (found.is_none() && range.crosses_edge(coords, 0.));
                found
            }),
        };
        *target = match found {
            Some(target_wisp) => TowerWispTarget::Wisp(target_wisp),
            // Wisps in cells crossed by the circle's boundary can enter range without changing the grid version.
            None if wisp_on_edge => TowerWispTarget::SearchForNewTarget,
            None => TowerWispTarget::NoValidTargets(wisps_grid.version),
        };
    }
}

fn tick_shooting_timers_system(
    time: Res<Time>,
    mut shooting_timers: Query<&mut TowerShootingTimer, With<IsOperational>>,
) {
    shooting_timers.iter_mut().for_each(|mut timer| { timer.0.tick(time.delta()); });
}

fn damage_control_system(
    mut commands: Commands,
    buildings: Query<(Entity, &IntegrityPoints), With<Building>>,
) {
    for (entity, integrity_points) in buildings.iter() {
        if integrity_points.is_dead() {
            commands.trigger(BuildingDestroyRequest(entity));
        }
    }
}

fn rotate_tower_top_system(
    towers: Query<(&TowerTopRotation, &TowerTop), (With<Tower>, Changed<TowerTopRotation>)>,
    mut tower_tops: Query<&mut Transform, With<TowerTopOf>>,
) {
    for (tower_top_rotation, tower_top) in towers.iter() {
        let Ok(mut tower_top_transform) = tower_tops.get_mut(tower_top.top()) else { continue };
        tower_top_transform.rotation = Quat::from_rotation_z(tower_top_rotation.current_angle);
    }
}

fn rotational_aiming_system(
    time: Res<Time>,
    mut towers: Query<(&mut TowerTopRotation, &TurnSpeed, &TowerWispTarget, &Transform), With<IsOperational>>,
    wisps: Query<&Transform, With<Wisp>>,
) {
    for (mut rotation, turn_speed, target, tower_transform) in towers.iter_mut() {
        let Some(Ok(wisp_transform)) = target.wisp().map(|wisp| wisps.get(wisp)) else { continue; };
        let angle_delta = rotation.angle_to(wisp_transform.translation.xy() - tower_transform.translation.xy());

        let rotation_delta = turn_speed.get() * time.delta_secs();
        let turn = angle_delta.clamp(-rotation_delta, rotation_delta);
        // Writing only an actual turn keeps `Changed<TowerTopRotation>` quiet for towers on target.
        if turn == 0. { continue; }
        rotation.current_angle += turn;
    }
}

#[log_tags(Tag::Build)]
fn on_building_destroy_request_do_so(
    trigger: On<BuildingDestroyRequest>,
    mut commands: Commands,
    almanach: Res<Almanach>,
    buildings: Query<(&GridArea, &BuildingType), With<Building>>,
) {
    let building_to_destroy = trigger.0;
    let Ok((area, building_type)) = buildings.get(building_to_destroy) else { return; };

    #[info_player("'{}' destroyed at {}", almanach.get_building_info(*building_type).name, area.origin)]
    commands.entity(building_to_destroy).despawn();
    commands.spawn_batch(area.iter().map(BuilderExplosion));
}
