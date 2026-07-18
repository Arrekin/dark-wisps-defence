use std::time::Duration;

use bevy::prelude::*;

use alteration::modifiers::prelude::*;
use almanach::prelude::*;
use buildings::prelude::*;
use game_core::{math::angle_difference, prelude::*};
use grids::{
    placement::{GridObjectPlacer, GridsCollectionParam, PlacementValidity, PlaceRequest},
    prelude::*,
    search::targetfinding::target_find_closest_wisp,
    wisps::WispsGrid,
};
use logging::prelude::*;
use resources::prelude::*;
use states::prelude::*;
use visuals::prelude::*;
use wisps::prelude::*;

use crate::common::*;

use super::{
    energy_relay::BuilderEnergyRelay,
    exploration_center::BuilderExplorationCenter,
    forge::BuilderForge,
    mining_complex::BuilderMiningComplex,
    tower_blaster::BuilderTowerBlaster,
    tower_cannon::BuilderTowerCannon,
    tower_emitter::BuilderTowerEmitter,
    tower_field::BuilderTowerField,
    tower_rocket_launcher::BuilderTowerRocketLauncher,
};

pub struct CommonSystemsPlugin;
impl Plugin for CommonSystemsPlugin {
    fn build(&self, app: &mut App) {
        app
            .add_systems(PreUpdate, (
                tick_shooting_timers_system,
                damage_control_system,
            ).run_if(in_state(GameState::Running)))
            .add_systems(Update,(
                (
                    targeting_system,
                    rotate_tower_top_system,
                    rotational_aiming_system,
                ).run_if(in_state(GameState::Running)),
            ))
            .add_observer(on_building_destroy_request_do_so)
            .add_observer(on_building_place_request_do_so)
            .add_observer(on_insert_attack_speed_sync_shooting_timer)
            ;
    }
}

fn on_insert_attack_speed_sync_shooting_timer(
    trigger: On<Insert, AttackSpeed>,
    mut timers: Query<(&mut TowerShootingTimer, &AttackSpeed)>,
) {
    let entity = trigger.entity;
    let Ok((mut timer, attack_speed)) = timers.get_mut(entity) else { return; };
    if attack_speed.get() == 0. { return; }
    timer.0.set_duration(Duration::from_secs_f32(1. / attack_speed.get()));
}

fn on_building_place_request_do_so(
    _trigger: On<PlaceRequest<Building>>,
    mut commands: Commands,
    almanach: Res<Almanach>,
    mut grids: GridsCollectionParam,
    mut stock: ResMut<Stock>,
    placer: Single<(&GridObjectPlacer, &GridCoords, &GridImprint)>,
    main_base: Query<Entity, With<MainBase>>,
) {
    let (grid_object_placer, coords, grid_imprint) = placer.into_inner();
    let Some(active_placement) = &grid_object_placer.active_placement else { return };
    let MapObject::Building(building_type) = active_placement.map_object else { return };

    let validity = (active_placement.placement_info.validate)(active_placement.map_object, *coords, *grid_imprint, &grids);
    if validity == PlacementValidity::Invalid { return; }

    // Payment
    let building_costs = &almanach.get_building_info(building_type).cost;
    if !stock.try_pay_costs(building_costs) { Log::info().player().tag(Tag::Build).message("Not enough resources"); return; }

    // Reserve and spawn
    grids.reserved_coords.reserve(*coords, *grid_imprint);
    Log::info().player().tag(Tag::Build).message(format!("'{}' placed at ({}, {})", almanach.get_building_info(building_type).name, coords.x, coords.y));
    match building_type {
        BuildingType::EnergyRelay => {
            commands.spawn(BuilderEnergyRelay::new(*coords));
        }
        BuildingType::ExplorationCenter => {
            commands.spawn(BuilderExplorationCenter::new(*coords));
        }
        BuildingType::Tower(TowerType::Blaster) => {
            commands.spawn(BuilderTowerBlaster::new(*coords));
        },
        BuildingType::Tower(TowerType::Cannon) => {
            commands.spawn(BuilderTowerCannon::new(*coords));
        },
        BuildingType::Tower(TowerType::RocketLauncher) => {
            commands.spawn(BuilderTowerRocketLauncher::new(*coords));
        },
        BuildingType::Tower(TowerType::Emitter) => {
            commands.spawn(BuilderTowerEmitter::new(*coords));
        },
        BuildingType::Tower(TowerType::Field) => {
            commands.spawn(BuilderTowerField::new(*coords));
        },
        BuildingType::MainBase => {
            let Ok(main_base_entity) = main_base.single() else { return; };
            // Remove/Insert ObstacleGridObject to trigger grid reprint
            commands.entity(main_base_entity).remove::<ObstacleGridObject>().insert(*coords).insert(ObstacleGridObject::Building);
        },
        BuildingType::MiningComplex => {
            commands.spawn(BuilderMiningComplex::new(*coords));
        },
        BuildingType::Forge => {
            commands.spawn(BuilderForge::new(*coords));
        },
    };
}

fn targeting_system(
    obstacle_grid: Res<ObstacleGrid>,
    wisps_grid: Res<WispsGrid>,
    mut towers: Query<(&GridCoords, &GridImprint, &AttackRange, &mut TowerWispTarget), (With<Tower>, With<IsOperational>)>,
    wisps: Query<&GridCoords, With<Wisp>>,
) {
    for (coords, grid_imprint, range, mut target) in towers.iter_mut() {
        match *target {
            TowerWispTarget::Wisp(wisp_entity) => {
                if let Ok(wisp_coords) = wisps.get(wisp_entity) {
                    // Check if wisp is still in range. For now we use Manhattan distance to check. This may not be correct for all tower types.
                    if coords.manhattan_distance(wisp_coords) <= range.get() as i32 { continue; }
                }
            },
            TowerWispTarget::NoValidTargets(grid_version) => {
                if grid_version == wisps_grid.version {
                    continue;
                }
            },
            TowerWispTarget::SearchForNewTarget => {},
        }
        if let Some((_a, target_wisp)) = target_find_closest_wisp(
            &obstacle_grid,
            &wisps_grid,
            grid_imprint.iter(*coords),
            range.get() as usize,
            true,
        ) {
            *target = TowerWispTarget::Wisp(target_wisp);
        } else {
            *target = TowerWispTarget::NoValidTargets(wisps_grid.version);
        }
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
    mut tower_rotational_top: Query<(&MarkerTowerRotationalTop, &mut Transform)>,
    towers: Query<&TowerTopRotation, With<Tower>>,
) {
    for (tower_rotational_top, mut tower_top_transform) in tower_rotational_top.iter_mut() {
        let parent_building = tower_rotational_top.0;
        let tower_top_rotation = towers.get(parent_building).unwrap();

        // Offset due to image naturally pointing downwards
        tower_top_transform.rotation = Quat::from_rotation_z(tower_top_rotation.current_angle);
    }
}

fn rotational_aiming_system(
    time: Res<Time>,
    mut towers: Query<(&mut TowerTopRotation, &TowerWispTarget, &Transform), With<IsOperational>>,
    wisps: Query<&Transform, With<Wisp>>,
) {
    for (mut rotation, target, tower_transform) in towers.iter_mut() {
        let TowerWispTarget::Wisp(target_wisp) = target else { continue; };
        let Ok(wisp_position) = wisps.get(*target_wisp).map(|target| target.translation.xy()) else { continue; };

        let direction_to_target = wisp_position - tower_transform.translation.xy();
        let target_angle = direction_to_target.y.atan2(direction_to_target.x);

        let angle_diff = angle_difference(target_angle, rotation.current_angle);

        let rotation_delta = rotation.speed * time.delta_secs();
        rotation.current_angle += angle_diff.clamp(-rotation_delta, rotation_delta);
    }
}

fn on_building_destroy_request_do_so(
    trigger: On<BuildingDestroyRequest>,
    mut commands: Commands,
    almanach: Res<Almanach>,
    buildings: Query<(&GridImprint, &GridCoords, &BuildingType), With<Building>>,
) {
    let building_to_destroy = trigger.0;
    let Ok((grid_imprint, grid_coords, building_type)) = buildings.get(building_to_destroy) else { return; };

    commands.entity(building_to_destroy).despawn();
    Log::info().player().tag(Tag::Build).message(format!("'{}' destroyed at ({}, {})", almanach.get_building_info(*building_type).name, grid_coords.x, grid_coords.y));
    grid_imprint.iter(*grid_coords).for_each(|coords| {
        commands.spawn(BuilderExplosion(coords));
    });
}