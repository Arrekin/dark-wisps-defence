use lib_grid::grids::obstacles::{ObstacleGrid, ReservedCoords};
use lib_grid::grids::wisps::WispsGrid;
use lib_grid::search::targetfinding::target_find_closest_wisp;
use lib_core::utils::angle_difference;

use crate::effects::explosions::BuilderExplosion;
use crate::prelude::*;
use crate::ui::grid_object_placer::GridObjectPlacer;
use crate::wisps::components::Wisp;
use super::{
    energy_relay::BuilderEnergyRelay,
    exploration_center::BuilderExplorationCenter,
    mining_complex::BuilderMiningComplex,
    tower_blaster::BuilderTowerBlaster,
    tower_emitter::BuilderTowerEmitter,
    tower_cannon::BuilderTowerCannon,
    tower_rocket_launcher::BuilderTowerRocketLauncher,
};

pub struct CommonSystemsPlugin;
impl Plugin for CommonSystemsPlugin {
    fn build(&self, app: &mut App) {
        app
            .add_systems(PreUpdate, tick_shooting_timers_system.run_if(in_state(GameState::Running)))
            .add_systems(Update,(
                (
                    targeting_system,
                    rotate_tower_top_system,
                    rotational_aiming_system,
                    damage_control_system,
                ).run_if(in_state(GameState::Running)),
            ))
            .add_observer(on_building_destroy_request)
            .add_observer(on_building_place_request)
            ;
    }
}

fn on_building_place_request(
    _trigger: On<lib_core::placement::PlaceRequest<Building>>,
    mut commands: Commands,
    mut reserved_coords: ResMut<ReservedCoords>,
    obstacle_grid: Res<ObstacleGrid>,
    almanach: Res<Almanach>,
    mut stock: ResMut<Stock>,
    placer: Single<(&GridObjectPlacer, &GridCoords, &GridImprint)>,
    main_base: Query<Entity, With<MainBase>>,
) {
    let (grid_object_placer, coords, grid_imprint) = placer.into_inner();
    let Some(active_placement) = &grid_object_placer.active_placement else { return };
    let MapObject::Building(building_type) = active_placement.map_object else { return };
    
    // Full validation
    if !coords.is_imprint_in_bounds(grid_imprint, obstacle_grid.bounds())
        || !obstacle_grid.query_building_placement(*coords, building_type, *grid_imprint) 
        || reserved_coords.any_reserved(*coords, *grid_imprint) { return; }
    
    // Payment
    let building_costs = &almanach.get_building_info(building_type).cost;
    if !stock.try_pay_costs(building_costs) { println!("Not enough dark ore"); return; }
    
    // Reserve and spawn
    reserved_coords.reserve(*coords, *grid_imprint);
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
        BuildingType::MainBase => {
            let Ok(main_base_entity) = main_base.single() else { return; };
            // Remove/Insert ObstacleGridObject to trigger grid reprint
            commands.entity(main_base_entity).remove::<ObstacleGridObject>().insert(*coords).insert(ObstacleGridObject::Building);
        },
        BuildingType::MiningComplex => {
            commands.spawn(BuilderMiningComplex::new(*coords));
        },
    };
}

fn targeting_system(
    obstacle_grid: Res<ObstacleGrid>,
    wisps_grid: Res<WispsGrid>,
    mut towers: Query<(&GridCoords, &GridImprint, &AttackRange, &mut TowerWispTarget), (With<Tower>, With<HasPower>, Without<DisabledByPlayer>)>,
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
            grid_imprint.covered_coords(*coords),
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
    mut shooting_timers: Query<&mut TowerShootingTimer, (With<HasPower>, Without<DisabledByPlayer>)>,
    time: Res<Time>,
) {
    shooting_timers.iter_mut().for_each(|mut timer| { timer.0.tick(time.delta()); });
}

fn damage_control_system(
    mut commands: Commands,
    buildings: Query<(Entity, &Health), With<Building>>,
) {
    for (entity, health) in buildings.iter() {
        if health.is_dead() {
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
    mut towers: Query<(&mut TowerTopRotation, &TowerWispTarget, &Transform), (With<HasPower>, Without<DisabledByPlayer>)>,
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

fn on_building_destroy_request(
    trigger: On<BuildingDestroyRequest>,
    mut commands: Commands,
    buildings: Query<(&GridImprint, &GridCoords), With<Building>>,
) {
    let building_to_destroy = trigger.0;
    let Ok((grid_imprint, grid_coords)) = buildings.get(building_to_destroy) else { return; };

    commands.entity(building_to_destroy).despawn();
    grid_imprint.covered_coords(*grid_coords).into_iter().for_each(|coords| {
        commands.spawn(BuilderExplosion(coords));
    });
    commands.queue(BuildingDestroyedMessage(building_to_destroy));
}