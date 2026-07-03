//! # Expedition Drone System
//!
//! Autonomous scanning drones deployed from Exploration Centers to scan ExpeditionZone targets.
//!
//! ## Architecture
//!
//! Each drone is a separate entity owned by an ExplorationCenter (via HomeBase relationship).
//! The drone-to-target relationship is tracked via `mission_target` field, not a component—this
//! allows the mission to persist across state transitions (e.g., returning→refueling→redeploying).
//!
//! ## State Machine
//!
//! ```text
//!                    ┌─────────────┐
//!        deploy      │  Stationed  │
//!       request      └──────┬──────┘
//!                           │
//!                           ▼
//!                    ┌─────────────┐
//!                    │  Deploying  │──┐
//!                    └──────┬──────┘  │
//!                           │ arrival │
//!                           ▼         │ fuel empty
//!                    ┌─────────────┐  │ or recall
//!                    │  Scanning   │──┤
//!                    └──────┬──────┘  │
//!                           │         │
//!                           ▼         │
//!                    ┌─────────────┐◄─┘
//!                    │  Returning  │
//!                    └──────┬──────┘
//!                           │ arrival at home
//!                           ▼
//!                    ┌─────────────┐
//!                    │  Refueling  │───► Deploying (if mission_target exists)
//!                    └─────────────┘───► Stationed (if no mission_target)
//! ```
//!
//! ## Fuel Design
//!
//! - Fuel depletes during Deploying and Scanning states only
//! - Return flight is "free" (no fuel cost)—ensures drones always make it home
//! - Refueling only occurs when home base (ExplorationCenter) is powered and enabled
//! - After refueling, drone auto-redeploys if `mission_target` is still set
//!
//! ## Scanning Mechanics
//!
//! During Scanning, drones orbit the target at PATROL_RADIUS. When the target is within
//! SCAN_ANGLE_LIMIT of the drone's heading, the beam activates and accumulates progress
//! on the target's ExpeditionZone component. The target consumes this progress for its
//! own mechanics (e.g., QuantumField layer progression).
//!
//! ## Visual Components
//!
//! - **ScanSpot**: Ground projection that wanders within target bounds
//! - **ScanningBeam**: Tapered cone from drone to spot (shader-based)

use bevy::{
    prelude::*,
    render::render_resource::AsBindGroup,
    shader::ShaderRef,
    sprite_render::{AlphaMode2d, Material2d, Material2dPlugin},
};

use buildings::prelude::{DisabledByPlayer, ExplorationCenter};
use game_core::{
    math::angle_difference,
    prelude::{GridCoords, GridImprint, Z_AERIAL_UNIT, Z_GROUND_EFFECT},
};
use grids::prelude::HasPower;
use map_objects::prelude::ExpeditionZone;
use persistence::prelude::{AppGameLoadSaveExtension, SaveableBatchCommand};
use states::prelude::{GameState, MapLoadingStage};
use units::{
    expedition_drone::{
        BEAM_START_WIDTH, BuilderExpeditionDrone, DRONE_FRONT_OFFSET, DRONE_SPEED,
        DRONE_TURN_RATE, DroneFuel, DroneState, EXPEDITION_DRONE_BASE_IMAGE,
        ExpeditionDrone, ExpeditionDroneDeploymentRequest, FUEL_CONSUMPTION_RATE,
        PATROL_RADIUS, REFUEL_RATE, RecallDrone, SCAN_ANGLE_LIMIT, SCAN_POINT_SPEED,
        SCAN_PROGRESS_RATE, ScanSpot, ScanningBeam, SPOT_ELONGATION_FACTOR, SPOT_RADIUS,
        WAYPOINT_REACH_DIST,
    },
    prelude::*,
};

pub struct ExpeditionDronePlugin;
impl Plugin for ExpeditionDronePlugin {
    fn build(&self, app: &mut App) {
        app
            .add_plugins((
                Material2dPlugin::<ScanningBeamMaterial>::default(),
                Material2dPlugin::<ScanSpotMaterial>::default(),
            ))
            .add_systems(Update, (
                (
                    refueling_system,
                    travel_system,
                    fuel_consumption_system,
                    (
                        patrol_system,
                        scan_spot_update,
                        scanning_beam_update,
                        zone_scan_progress_system,
                    ).chain(),
                ).run_if(in_state(GameState::Running)),
            ))
            .add_observer(on_builder_add_spawn_expedition_drone)
            .add_observer(on_deployment_request_drone_do_so)
            .add_observer(on_recall_drone_do_so)
            .add_observer(on_state_changed_handle_drone_state_change)
            .register_db_loader::<BuilderExpeditionDrone>(MapLoadingStage::SpawnMapElements)
            .register_db_saver(on_game_save_collect_expedition_drones);
    }
}

/// Refuels drones at home base. Transitions to Deploying when full (auto-redeploy).
/// The state observer handles the Deploying→Stationed fallback if mission_target is None.
fn refueling_system(
    mut commands: Commands,
    time: Res<Time>,
    mut drones: Query<(Entity, &DroneState, &mut DroneFuel, &HomeBase), With<ExpeditionDrone>>,
    exploration_centers: Query<(), (With<ExplorationCenter>, With<HasPower>, Without<DisabledByPlayer>)>,
) {
    for (entity, drone_state, mut drone_fuel, home_base) in drones.iter_mut() {
        if !matches!(drone_state, DroneState::Refueling) { continue; }
        
        // Only refuel if home base is operational
        if !exploration_centers.contains(home_base.0) { continue; }
        
        // Refuel over time
        drone_fuel.refuel(REFUEL_RATE * time.delta_secs());
        
        // When full, redeploy. If there is no mission, reinsertion observer will check for that and set it to Stationed
        if drone_fuel.is_full() {
            commands.entity(entity).insert(DroneState::Deploying);
        }
    }
}

/// Initiates deployment: sets mission_target, positions at home base, and transitions to Deploying.
fn on_deployment_request_drone_do_so(
    trigger: On<ExpeditionDroneDeploymentRequest>,
    mut commands: Commands,
    mut drones: Query<(&mut Transform, &DroneState, &mut ExpeditionDrone, &HomeBase)>,
    home_bases: Query<&Transform, (With<ExplorationCenter>, Without<ExpeditionDrone>)>,
    exploration_centers: Query<(), (With<ExplorationCenter>, With<HasPower>, Without<DisabledByPlayer>)>,
    targets: Query<&Transform, Without<ExpeditionDrone>>,
) {
    let event = trigger.event();
    let Ok((mut transform, drone_state, mut drone, home_base)) = drones.get_mut(event.drone) else { return };
    
    // Only deploy if home base is operational
    if !exploration_centers.contains(home_base.0) { return; }
    
    // Only stationed drones can be sent out
    if !matches!(drone_state, DroneState::Stationed) { return; }

    drone.mission_target = Some(event.target);

    let Ok(home_transform) = home_bases.get(home_base.0) else { return; };
    let Ok(target_transform) = targets.get(event.target) else { return; };
    
    let home_pos = home_transform.translation.xy();
    
    transform.translation = home_pos.extend(transform.translation.z);
    
    // Point toward target
    let to_target = target_transform.translation.xy() - home_pos;
    drone.heading = to_target.y.atan2(to_target.x);
    transform.rotation = Quat::from_rotation_z(drone.heading);

    commands.entity(event.drone).insert(DroneState::Deploying);
}

fn on_state_changed_handle_drone_state_change(
    trigger: On<Insert, DroneState>,
    mut commands: Commands,
    mut drones: Query<(&DroneState, &mut ExpeditionDrone, &mut Visibility)>,
    targets: Query<&Transform>,
) {
    let drone_entity = trigger.entity;
    let Ok((drone_state, mut drone, mut visibility)) = drones.get_mut(drone_entity) else { return; };
    
    match drone_state {
        DroneState::Stationed => {
            *visibility = Visibility::Hidden;
        }
        DroneState::Refueling => {
            *visibility = Visibility::Hidden;
        }
        DroneState::Deploying => {
            if drone.mission_target.is_none() {
                commands.entity(drone_entity).insert(DroneState::Stationed);
                return;
            };
            
            *visibility = Visibility::Inherited;
        }
        DroneState::Scanning => {
            let mut rng = nanorand::tls_rng();

            let Some(target_entity) = drone.mission_target else {
                commands.entity(drone_entity).insert(DroneState::Returning);
                return;
            };
            let Ok(target_transform) = targets.get(target_entity) else { return; };
            drone.set_new_waypoint(target_transform.translation.xy(), &mut rng);
        }
        _ => {}
    }
    
}

/// Manual recall: sends drone home and clears mission_target.
/// Clearing mission ensures drone goes to Stationed after refueling, not auto-redeploy.
fn on_recall_drone_do_so(
    trigger: On<RecallDrone>,
    mut commands: Commands,
    mut drones: Query<(&DroneState, &mut ExpeditionDrone)>,
) {
    let drone_entity = trigger.0;
    let Ok((drone_state, mut drone)) = drones.get_mut(drone_entity) else { return; };
    
    match drone_state {
        DroneState::Deploying | DroneState::Scanning => {
            commands.entity(drone_entity).insert(DroneState::Returning);
            drone.mission_target = None; // Clear mission on manual recall
        }
        DroneState::Refueling | DroneState::Returning => {
            // Clear mission so when refuelling is done it will go into stationed mode
            drone.mission_target = None;
        }
        _ => {}
    }
}

/// Moves drones toward destination with smooth turning (DRONE_TURN_RATE).
fn travel_system(
    mut commands: Commands,
    time: Res<Time>,
    mut drones: Query<(Entity, &DroneState, &mut ExpeditionDrone, &mut Transform, Option<&HomeBase>)>,
    home_bases: Query<&Transform, (With<ExplorationCenter>, Without<ExpeditionDrone>)>,
    targets: Query<&Transform, (With<ExpeditionZone>, Without<ExplorationCenter>, Without<ExpeditionDrone>)>,
) {
    for (entity, drone_state, mut drone, mut transform, maybe_home_base) in drones.iter_mut() {
        let (destination, arrival_dist) = match drone_state {
            DroneState::Deploying => {
                let Some(target_entity) = drone.mission_target else { continue; };
                let Ok(target_transform) = targets.get(target_entity) else {
                    commands.entity(entity).insert(DroneState::Returning);
                    continue; 
                };
                (target_transform.translation.xy(), PATROL_RADIUS)
            }
            DroneState::Returning => {
                let Some(home_base) = maybe_home_base else {
                    commands.entity(entity).despawn();
                    continue;
                };
                let Ok(home_transform) = home_bases.get(home_base.0) else { continue; };
                (home_transform.translation.xy(), 10.0)
            }
            _ => continue,
        };
        
        let drone_pos = transform.translation.xy();
        let to_dest = destination - drone_pos;
        let distance = to_dest.length();
        
        // Check arrival
        if distance < arrival_dist {
            match drone_state {
                DroneState::Deploying => { commands.entity(entity).insert(DroneState::Scanning); }
                DroneState::Returning => { commands.entity(entity).insert(DroneState::Refueling); }
                _ => {}
            }
            continue;
        }
        
        // Fly toward destination
        let desired_heading = to_dest.y.atan2(to_dest.x);
        let heading_diff = angle_difference(desired_heading, drone.heading);
        let max_turn = DRONE_TURN_RATE * time.delta_secs();
        drone.heading += heading_diff.clamp(-max_turn, max_turn);
        
        let forward = Vec2::new(drone.heading.cos(), drone.heading.sin());
        let move_dist = (DRONE_SPEED * time.delta_secs()).min(distance);
        transform.translation += (forward * move_dist).extend(0.0);
        transform.rotation = Quat::from_rotation_z(drone.heading);
    }
}

/// Drains fuel during active mission. Return flight is free to guarantee drones reach home.
fn fuel_consumption_system(
    mut commands: Commands,
    time: Res<Time>,
    mut drones: Query<(Entity, &DroneState, &mut DroneFuel)>,
) {
    for (entity, drone_state, mut fuel) in drones.iter_mut() {
        match drone_state {
            DroneState::Deploying | DroneState::Scanning => {
                fuel.consume(FUEL_CONSUMPTION_RATE * time.delta_secs());

                if fuel.is_empty() {
                    // Fuel depleted - return but keep mission (will redeploy after refuel)
                    commands.entity(entity).insert(DroneState::Returning);
                }
            }
            _ => {}
        }
    }
}

/// Orbital patrol during Scanning: drone circles target, beam activates when target is ahead.
fn patrol_system(
    mut commands: Commands,
    time: Res<Time>,
    targets: Query<&Transform, Without<ExpeditionDrone>>,
    mut drones: Query<(Entity, &DroneState, &mut Transform, &mut ExpeditionDrone)>,
) {
    let mut rng = nanorand::tls_rng();
    
    for (entity, drone_state, mut transform, mut drone) in drones.iter_mut() {
        // Reset beam state each frame (will be set true if conditions met)
        drone.is_beam_active = false;
        
        if !matches!(drone_state,  DroneState::Scanning) { continue; }
        
        let Some(target_entity) = drone.mission_target else { continue; };
        let Ok(target_transform) = targets.get(target_entity) else {
            commands.entity(entity).insert(DroneState::Returning);
            continue;
        };
        let center = target_transform.translation.xy();
        let drone_pos = transform.translation.xy();
        
        // Check angle to target center
        let to_target = center - drone_pos;
        let angle_to_target = to_target.y.atan2(to_target.x);
        let angle_diff = angle_difference(angle_to_target, drone.heading);
        let target_in_front = angle_diff.abs() < SCAN_ANGLE_LIMIT;
        
        // Set beam active when target is in front
        drone.is_beam_active = target_in_front;
        
        // Determine desired heading based on whether target is in front
        let desired_heading = if target_in_front {
            angle_to_target
        } else {
            let to_waypoint = drone.waypoint - drone_pos;
            let dist_to_waypoint = to_waypoint.length();
            
            // Pick new waypoint when reached
            if dist_to_waypoint < WAYPOINT_REACH_DIST {
                drone.set_new_waypoint(center, &mut rng);
            }
            to_waypoint.y.atan2(to_waypoint.x)
        };
        
        let heading_diff = angle_difference(desired_heading, drone.heading);
        let max_turn = DRONE_TURN_RATE * time.delta_secs();
        drone.heading += heading_diff.clamp(-max_turn, max_turn);
        
        let forward = Vec2::new(drone.heading.cos(), drone.heading.sin());
        transform.translation += (forward * DRONE_SPEED * time.delta_secs()).extend(0.0);
        transform.rotation = Quat::from_rotation_z(drone.heading);
    }
}

/// Transfers scan progress to target's ExpeditionZone while beam is active.
fn zone_scan_progress_system(
    time: Res<Time>,
    drones: Query<&ExpeditionDrone>,
    mut zones: Query<&mut ExpeditionZone>,
) {
    for drone in drones.iter() {
        if !drone.is_beam_active { continue; }
        let Some(target_entity) = drone.mission_target else { continue; };
        let Ok(mut zone) = zones.get_mut(target_entity) else { continue; };
        zone.accumulated_scan_progress += SCAN_PROGRESS_RATE * time.delta_secs();
    }
}

/// Updates scanning beam position and appearance.
/// - Hides beam when drone beam is inactive
/// - Stretches beam from drone front to scan spot
/// - Animates pulse effect traveling down the beam
fn scanning_beam_update(
    mut commands: Commands,
    mut beam_materials: ResMut<Assets<ScanningBeamMaterial>>,
    time: Res<Time>,
    drones: Query<(&Transform, &ExpeditionDrone)>,
    spots: Query<&Transform, With<ScanSpot>>,
    mut beams: Query<(Entity, &mut Transform, &ScanningBeam, &MeshMaterial2d<ScanningBeamMaterial>), (Without<ExpeditionDrone>, Without<ScanSpot>)>,
) {
    for (beam_entity, mut beam_transform, beam, material_handle) in beams.iter_mut() {
        let Ok((drone_transform, drone)) = drones.get(beam.drone) else {
            // TODO: perhaps there will be a better way to handling on drone removed to delete beam and spot
            commands.entity(beam_entity).despawn();
            commands.entity(beam.spot).despawn();
            continue;
        };
        let Ok(spot_transform) = spots.get(beam.spot) else { unreachable!(); };
        
        if !drone.is_beam_active {
            beam_transform.scale = Vec3::ZERO;
            continue;
        }
        
        let drone_pos = drone_transform.translation.xy();
        let drone_forward = (drone_transform.rotation * Vec3::X).xy().normalize();
        let beam_start = drone_pos + drone_forward * DRONE_FRONT_OFFSET;
        let target_point = spot_transform.translation.xy();
        
        let beam_vec = target_point - beam_start;
        let beam_length = beam_vec.length();
        let beam_angle = beam_vec.y.atan2(beam_vec.x);
        
        let beam_center = (beam_start + target_point) / 2.0;
        beam_transform.translation = beam_center.extend(Z_GROUND_EFFECT);
        beam_transform.rotation = Quat::from_rotation_z(beam_angle);
        
        let spot_diameter = SPOT_RADIUS * 2.0;
        beam_transform.scale = Vec3::new(beam_length, spot_diameter, 1.0);
        
        if let Some(mut material) = beam_materials.get_mut(material_handle) {
            material.start_width = BEAM_START_WIDTH / spot_diameter;
            material.end_width = 1.0;
            material.pulse = (material.pulse + time.delta_secs() * 0.8) % 1.0;
        }
    }
}

/// Updates scan spot position and appearance.
/// - Hides spot when beam is inactive
/// - Snaps spot to target bounds if too far (e.g. after switching targets)
/// - Moves spot toward random destination within target bounds
/// - Elongates spot based on distance to drone (perspective effect)
fn scan_spot_update(
    mut spot_materials: ResMut<Assets<ScanSpotMaterial>>,
    time: Res<Time>,
    drones: Query<(&Transform, &ExpeditionDrone), Without<ScanSpot>>,
    targets: Query<(&GridCoords, &GridImprint), Without<ExpeditionDrone>>,
    beams: Query<&ScanningBeam>,
    mut spots: Query<(&mut Transform, &mut ScanSpot, &MeshMaterial2d<ScanSpotMaterial>), Without<ExpeditionDrone>>,
) {
    for beam in beams.iter() {
        let Ok((mut spot_transform, mut spot, material_handle)) = spots.get_mut(beam.spot) else { continue; };
        let Ok((drone_transform, drone)) = drones.get(beam.drone) else { continue; };
        
        if !drone.is_beam_active {
            spot_transform.scale = Vec3::ZERO;
            continue;
        }
        
        let Some(target_entity) = drone.mission_target else { continue; };
        let Ok((target_coords, target_imprint)) = targets.get(target_entity) else { continue; };
        
        let drone_pos = drone_transform.translation.xy();
        
        // Snap spot to target if too far (e.g. after switching targets)
        let spot_world_pos = {
            let spot_world_pos = spot_transform.translation.xy();
            let target_world_center = target_coords.to_world_position_centered(target_imprint);
            let dist_to_target_center = (spot_world_pos - target_world_center).length();
            let max_target_dist = target_imprint.world_size().length() * 0.75;
            if dist_to_target_center > max_target_dist {
                let new_pos = target_coords.to_world_position() + target_imprint.random_local_offset();
                spot.destination = new_pos;
                spot_transform.translation = new_pos.extend(spot_transform.translation.z);
                new_pos
            } else {
                spot_world_pos
            }
        };
        
        // Move spot toward destination
        let to_destination = spot.destination - spot_world_pos;
        let distance_to_dest = to_destination.length();
        
        if distance_to_dest < 2.0 {
            spot.destination = target_coords.to_world_position() + target_imprint.random_local_offset();
        } else {
            let move_amount = SCAN_POINT_SPEED * time.delta_secs();
            let new_world_pos = if move_amount >= distance_to_dest {
                spot.destination
            } else {
                spot_world_pos + to_destination.normalize() * move_amount
            };
            spot_transform.translation = new_world_pos.extend(spot_transform.translation.z);
        }
        
        // Elongate spot based on distance to drone (perspective effect)
        let to_drone = drone_pos - spot_transform.translation.xy();
        let distance = to_drone.length();
        let angle_to_drone = to_drone.y.atan2(to_drone.x);
        spot_transform.rotation = Quat::from_rotation_z(angle_to_drone);
        let elongation = 1.0 + distance * SPOT_ELONGATION_FACTOR;
        spot_transform.scale = Vec3::new(SPOT_RADIUS * elongation, SPOT_RADIUS, 1.0);
        
        if let Some(mut material) = spot_materials.get_mut(material_handle) {
            material.pulse = (material.pulse + time.delta_secs() * 0.8) % 1.0;
        }
    }
}


/// Shader material for the tapered beam effect. Width interpolates from narrow (drone) to wide (spot).
#[derive(Asset, TypePath, Debug, Clone, AsBindGroup)]
pub(crate) struct ScanningBeamMaterial {
    #[uniform(0)]
    pub start_width: f32,  // width at drone end (narrow)
    #[uniform(0)]
    pub end_width: f32,    // width at spot end (wide)
    #[uniform(0)]
    pub pulse: f32,        // animation 0-1
}
impl Default for ScanningBeamMaterial {
    fn default() -> Self {
        Self { start_width: 0.1, end_width: 0.4, pulse: 0.0 }
    }
}

impl Material2d for ScanningBeamMaterial {
    fn fragment_shader() -> ShaderRef {
        "shaders/scanning_beam.wgsl".into()
    }
    fn alpha_mode(&self) -> AlphaMode2d {
        AlphaMode2d::Blend
    }
}

/// Material for the scan spot (ground projection).
#[derive(Asset, TypePath, Debug, Clone, AsBindGroup, Default)]
pub(crate) struct ScanSpotMaterial {
    #[uniform(0)]
    pub pulse: f32,  // animation 0-1
}

impl Material2d for ScanSpotMaterial {
    fn fragment_shader() -> ShaderRef {
        "shaders/scan_spot.wgsl".into()
    }
    fn alpha_mode(&self) -> AlphaMode2d {
        AlphaMode2d::Blend
    }
}

fn on_game_save_collect_expedition_drones(
    mut commands: Commands,
    drones: Query<(Entity, &ExpeditionDrone, &DroneState, &HomeBase, &DroneFuel, &Transform)>,
) {
    if drones.is_empty() { return; }
    let batch = drones.iter().map(|(entity, drone, drone_state, home_base, fuel, transform)| {
        BuilderExpeditionDrone::new_for_saving(drone, drone_state, home_base.0, fuel, transform, entity)
    }).collect::<SaveableBatchCommand<_>>();
    commands.queue(batch);
}

/// Spawns drone with linked visual components (ScanSpot + ScanningBeam as separate entities).
/// Visual entities hold references to drone; beam also references spot for positioning.
fn on_builder_add_spawn_expedition_drone(
    trigger: On<Add, BuilderExpeditionDrone>,
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut beam_materials: ResMut<Assets<ScanningBeamMaterial>>,
    mut spot_materials: ResMut<Assets<ScanSpotMaterial>>,
    builders: Query<&BuilderExpeditionDrone>,
    home_bases: Query<&Transform, With<ExplorationCenter>>,
) {
    let entity = trigger.entity;
    let Ok(builder) = builders.get(entity) else { return; };
    
    // Extract data - use defaults for new drones, saved data for loaded ones
    let (state, mission_target, world_position, heading, waypoint, fuel_current, fuel_max) = 
        if let Some(data) = &builder.save_data {
            (data.state, data.mission_target, data.world_position, data.heading, data.waypoint, data.fuel_current, data.fuel_max)
        } else {
            // New drone - get home position
            let home_pos = home_bases.get(builder.home_base)
                .map(|t| t.translation.xy())
                .unwrap_or(Vec2::ZERO);
            (DroneState::Stationed, None, home_pos, 0.0, Vec2::ZERO, 60.0, 60.0)
        };
    
    // For stationed drones, position at home base (hidden)
    let (final_position, visibility) = if state == DroneState::Stationed {
        let home_pos = home_bases.get(builder.home_base)
            .map(|t| t.translation.xy())
            .unwrap_or(world_position);
        (home_pos, Visibility::Hidden)
    } else {
        (world_position, Visibility::Inherited)
    };
    
    // Create scanning visual materials
    let beam_mesh = meshes.add(Rectangle::new(1.0, 1.0));
    let beam_material = beam_materials.add(ScanningBeamMaterial::default());
    let spot_mesh = meshes.add(Circle::new(1.0));
    let spot_material = spot_materials.add(ScanSpotMaterial::default());
    
    // Spawn scan spot
    let spot_entity = commands.spawn((
        Mesh2d(spot_mesh),
        MeshMaterial2d(spot_material),
        Transform {
            translation: final_position.extend(Z_GROUND_EFFECT),
            scale: Vec3::ZERO, // Start hidden
            ..default()
        },
        ScanSpot {
            destination: final_position,
        },
    )).id();
    
    // Spawn beam
    commands.spawn((
        Mesh2d(beam_mesh),
        MeshMaterial2d(beam_material),
        Transform::from_translation(Vec3::new(0.0, 0.0, Z_GROUND_EFFECT)),
        ScanningBeam {
            drone: entity,
            spot: spot_entity,
        },
    ));
    
    commands.entity(entity)
        .remove::<BuilderExpeditionDrone>()
        .insert((
            Sprite {
                image: asset_server.load(EXPEDITION_DRONE_BASE_IMAGE),
                ..default()
            },
            Transform {
                translation: final_position.extend(Z_AERIAL_UNIT),
                scale: Vec3::new(1.5, 1.5, 1.0),
                rotation: Quat::from_rotation_z(heading),
            },
            visibility,
            ExpeditionDrone {
                mission_target,
                heading,
                waypoint,
                is_beam_active: false,
            },
            state,
            HomeBase(builder.home_base),
            DroneFuel {
                current: fuel_current,
                max: fuel_max,
            },
            Pickable{ should_block_lower: false, is_hoverable: true },
        ));
}
