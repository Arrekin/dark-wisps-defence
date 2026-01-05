//! # Expedition Drone System
//!
//! Autonomous scanning drones deployed from Exploration Centers to gather intel on targets.
//!
//! ## Lifecycle
//! 1. **Stationed** - Drone is hidden at home base, refueling
//! 2. **Deploying** - Drone flies toward assigned target (ExpeditionTargetMarker)
//! 3. **Scanning** - Drone patrols around target, activating scan beam when target is in view
//! 4. **Returning** - Drone flies back to home base (triggered by fuel depletion or target loss)
//!
//! ## Visual Components
//! - **ScanSpot**: Ground projection showing where the scan beam hits
//! - **ScanningBeam**: Cone of light connecting drone to scan spot
//!
//! ## Fuel Mechanics
//! Drones consume fuel while deploying or scanning. When fuel is depleted, they return home.
//! Fuel is fully restored when the drone arrives back at the home base.

use bevy::{
    render::render_resource::AsBindGroup, 
    shader::ShaderRef,
    sprite_render::{AlphaMode2d, Material2d, Material2dPlugin}
};
use lib_core::utils::angle_difference;

use crate::prelude::*;
use crate::map_objects::common::ExpeditionZone;

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
                    ExpeditionDrone::refueling_system,
                    ExpeditionDrone::travel_system,
                    ExpeditionDrone::fuel_consumption_system,
                    (
                        ExpeditionDrone::patrol_system,
                        ScanSpot::update,
                        ScanningBeam::update,
                    ).chain(),
                ).run_if(in_state(GameState::Running)),
            ))
            .add_observer(BuilderExpeditionDrone::on_add)
            .add_observer(ExpeditionDrone::on_deployment_request)
            .add_observer(ExpeditionDrone::on_recall)
            .add_observer(ExpeditionDrone::on_state_changed)
            .register_db_loader::<BuilderExpeditionDrone>(MapLoadingStage::SpawnMapElements)
            .register_db_saver(BuilderExpeditionDrone::on_game_save);
    }
}

pub const EXPEDITION_DRONE_BASE_IMAGE: &str = "units/expedition_drone.png";
const PATROL_RADIUS: f32 = 150.0;           // how far waypoints spawn from target center
const DRONE_SPEED: f32 = 160.0;             // world units per second
const DRONE_TURN_RATE: f32 = 1.2;           // radians per second (how fast it can turn)
const WAYPOINT_REACH_DIST: f32 = 2.0;       // how close before picking new waypoint
const DRONE_FRONT_OFFSET: f32 = 32.0;       // pixels from drone center to front
const SCAN_ANGLE_LIMIT: f32 = 1.6;          // radians (~90°) - max angle from forward to scan
const SCAN_POINT_SPEED: f32 = 20.0;         // world units per second - how fast beam moves to target
const SPOT_RADIUS: f32 = 25.0;              // base radius of scan spot on ground
const SPOT_ELONGATION_FACTOR: f32 = 0.0015; // elongation per unit distance (0 = circle when close)
const BEAM_START_WIDTH: f32 = 2.0;          // width at drone (narrow apex)
const DEFAULT_MAX_FUEL: f32 = 60.0;         // seconds of flight time
pub const FUEL_CONSUMPTION_RATE: f32 = 3.0;     // fuel units per second while on mission
pub const REFUEL_RATE: f32 = 15.0;              // fuel units per second while refueling

/// Drone cost in dark ore - kept as constant for easy balancing
pub const DRONE_COST_ORE: u32 = 100;

/// Request to deploy a drone to a specific target
#[derive(Event)]
pub struct ExpeditionDroneDeploymentRequest {
    pub drone: Entity,
    pub target: Entity,
}

/// Event to recall a drone back to base
#[derive(EntityEvent)]
pub struct RecallDrone(pub Entity);

#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq, strum::Display)]
#[component(immutable)]
pub enum DroneState {
    #[default]
    Stationed,      // Hidden at home base, no active mission
    Refueling,      // At home base, refueling before redeploying (has mission_target)
    Deploying,      // Flying toward mission target
    Scanning,       // Patrolling and scanning target
    Returning,      // Flying back to home base
}

#[derive(Component)]
#[require(MapBound)]
pub struct ExpeditionDrone {
    pub mission_target: Option<Entity>, // current mission target (ExpeditionZone)
    pub heading: f32,             // current facing direction in radians
    pub waypoint: Vec2,           // current waypoint we're flying toward
    pub is_beam_active: bool,     // true when target is in front and beam should show
}
impl ExpeditionDrone {
    /// Generate a new patrol waypoint on the opposite side of the target
    fn set_new_waypoint(&mut self, target_center: Vec2, rng: &mut nanorand::tls::TlsWyRand) {
        use nanorand::Rng;
        let overshoot_angle = self.heading + std::f32::consts::PI;
        let angle_variation = (rng.generate::<f32>() - 0.5) * std::f32::consts::FRAC_PI_4;
        let waypoint_angle = overshoot_angle + angle_variation;
        self.waypoint = target_center + Vec2::new(
            PATROL_RADIUS * waypoint_angle.cos(),
            PATROL_RADIUS * waypoint_angle.sin(),
        );
    }

    /// Refueling system - handles drones in Refueling state
    /// Refuels drone over time, then either redeploys or goes to stationed
    fn refueling_system(
        mut commands: Commands,
        time: Res<Time>,
        mut drones: Query<(Entity, &DroneState, &mut DroneFuel), With<ExpeditionDrone>>,
    ) {
        for (entity, drone_state, mut drone_fuel) in drones.iter_mut() {
            if !matches!(drone_state, DroneState::Refueling) { continue; }
            
            // Refuel over time
            drone_fuel.refuel(REFUEL_RATE * time.delta_secs());
            
            // When full, redeploy. If there is no mission, reinsertion observer will check for that and set it to Stationed
            if drone_fuel.is_full() {
                commands.entity(entity).insert(DroneState::Deploying);
            }
        }
    }
    
    /// Handle ExpeditionDroneDeploymentRequest event - deploys a stationed drone to a target
    fn on_deployment_request(
        trigger: On<ExpeditionDroneDeploymentRequest>,
        mut commands: Commands,
        mut drones: Query<(&mut Transform, &DroneState, &mut ExpeditionDrone, &HomeBase)>,
        home_bases: Query<&Transform, (With<ExplorationCenter>, Without<ExpeditionDrone>)>,
        targets: Query<&Transform, Without<ExpeditionDrone>>,
    ) {
        let event = trigger.event();
        let Ok((mut transform, drone_state, mut drone, home_base)) = drones.get_mut(event.drone) else { return };
        
        // Only statione drones can be sent out
        if !matches!(drone_state, DroneState::Stationed) { return; }
    
        drone.mission_target = Some(event.target);

        let Ok(home_transform) = home_bases.get(home_base.0) else { return };
        let Ok(target_transform) = targets.get(event.target) else { return };
        
        let home_pos = home_transform.translation.xy();
        
        transform.translation = home_pos.extend(transform.translation.z);
        
        // Point toward target
        let to_target = target_transform.translation.xy() - home_pos;
        drone.heading = to_target.y.atan2(to_target.x);
        transform.rotation = Quat::from_rotation_z(drone.heading);

        commands.entity(event.drone).insert(DroneState::Deploying);
    }

    fn on_state_changed(
        trigger: On<Insert, DroneState>,
        mut commands: Commands,
        mut drones: Query<(&DroneState, &mut ExpeditionDrone, &mut Visibility)>,
        targets: Query<&Transform>,
    ) {
        let drone_entity = trigger.entity;
        let Ok((drone_state, mut drone, mut visibility)) = drones.get_mut(drone_entity) else { return };
        
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
                let Ok(target_transform) = targets.get(target_entity) else { return };
                drone.set_new_waypoint(target_transform.translation.xy(), &mut rng);
            }
            _ => {}
        }
        
    }
    
    /// Handle RecallDrone event - sends drone back to base and clears mission
    fn on_recall(
        trigger: On<RecallDrone>,
        mut commands: Commands,
        mut drones: Query<(&DroneState, &mut ExpeditionDrone)>,
    ) {
        let drone_entity = trigger.0;
        let Ok((drone_state, mut drone)) = drones.get_mut(drone_entity) else { return };
        
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

    /// Moves drones in Deploying or Returning states toward their destination.
    /// - Deploying: flies toward mission target, transitions to Scanning on arrival
    /// - Returning: flies toward home base, transitions to Stationed on arrival (refuels, hides)
    /// - Handles smooth turning with DRONE_TURN_RATE limit
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

    /// Consumes fuel while drone is on mission and triggers return when depleted.
    /// - Drains fuel during Deploying and Scanning states
    /// - Triggers Returning state if fuel empty
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

    /// Handles patrol movement and beam activation during Scanning state.
    /// - Resets is_beam_active each frame
    /// - Activates beam when target is within SCAN_ANGLE_LIMIT of drone heading
    /// - When target in front: turns toward target center
    /// - When target not in front: flies toward current waypoint, picks new one when reached
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

}

#[derive(Component)]
#[require(MapBound)]
pub struct ScanningBeam {
    pub drone: Entity,
    pub spot: Entity,             // the scan spot entity
}
impl ScanningBeam {
    /// Updates scanning beam position and appearance.
    /// - Hides beam when drone beam is inactive
    /// - Stretches beam from drone front to scan spot
    /// - Animates pulse effect traveling down the beam
    fn update(
        time: Res<Time>,
        drones: Query<(&Transform, &ExpeditionDrone)>,
        spots: Query<&Transform, With<ScanSpot>>,
        mut beams: Query<(&mut Transform, &ScanningBeam, &MeshMaterial2d<ScanningBeamMaterial>), (Without<ExpeditionDrone>, Without<ScanSpot>)>,
        mut beam_materials: ResMut<Assets<ScanningBeamMaterial>>,
    ) {
        for (mut beam_transform, beam, material_handle) in beams.iter_mut() {
            let Ok((drone_transform, drone)) = drones.get(beam.drone) else { continue; };
            let Ok(spot_transform) = spots.get(beam.spot) else { continue; };
            
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
            
            if let Some(material) = beam_materials.get_mut(material_handle) {
                material.start_width = BEAM_START_WIDTH / spot_diameter;
                material.end_width = 1.0;
                material.pulse = (material.pulse + time.delta_secs() * 0.8) % 1.0;
            }
        }
    }
}

#[derive(Component)]
#[require(MapBound)]
pub struct ScanSpot {
    pub destination: Vec2,        // where moving toward (world coords)
}
impl ScanSpot {
    /// Updates scan spot position and appearance.
    /// - Hides spot when beam is inactive
    /// - Snaps spot to target bounds if too far (e.g. after switching targets)
    /// - Moves spot toward random destination within target bounds
    /// - Elongates spot based on distance to drone (perspective effect)
    fn update(
        time: Res<Time>,
        drones: Query<(&Transform, &ExpeditionDrone), Without<ScanSpot>>,
        targets: Query<(&GridCoords, &GridImprint), Without<ExpeditionDrone>>,
        beams: Query<&ScanningBeam>,
        mut spots: Query<(&mut Transform, &mut ScanSpot, &MeshMaterial2d<ScanSpotMaterial>), Without<ExpeditionDrone>>,
        mut spot_materials: ResMut<Assets<ScanSpotMaterial>>,
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
            
            if let Some(material) = spot_materials.get_mut(material_handle) {
                material.pulse = (material.pulse + time.delta_secs() * 1.2) % 1.0;
            }
        }
    }
}

#[derive(Component)]
pub struct DroneFuel {
    pub current: f32,
    pub max: f32,
}
impl Default for DroneFuel {
    fn default() -> Self {
        Self { current: DEFAULT_MAX_FUEL, max: DEFAULT_MAX_FUEL }
    }
}
impl DroneFuel {
    pub fn is_empty(&self) -> bool { self.current <= 0.0 }
    pub fn is_full(&self) -> bool { self.current >= self.max }
    pub fn fraction(&self) -> f32 { self.current / self.max }
    pub fn refuel(&mut self, amount: f32) { self.current = (self.current + amount).min(self.max); }
    pub fn consume(&mut self, amount: f32) { self.current = (self.current - amount).max(0.0); }
    
    /// Calculate fuel cost to travel a distance (deployment only, return is free)
    pub fn travel_fuel_cost(distance: f32) -> f32 {
        let travel_time = distance / DRONE_SPEED;
        travel_time * FUEL_CONSUMPTION_RATE
    }
    
    /// Calculate fuel percentage needed to reach target from a position
    pub fn fuel_percent_for_distance(&self, distance: f32) -> f32 {
        if self.current <= 0.0 { return f32::INFINITY; }
        let cost = Self::travel_fuel_cost(distance);
        (cost / self.current) * 100.0
    }
}


/// Material for the scanning beam (cone from drone to spot).
#[derive(Asset, TypePath, Debug, Clone, AsBindGroup)]
pub struct ScanningBeamMaterial {
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
pub struct ScanSpotMaterial {
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

////////////////////////////////////////////
////       Builder / Persistence        ////
////////////////////////////////////////////

#[derive(Clone, Copy, Debug)]
pub struct DroneSaveData {
    pub entity: Entity,
    pub state: DroneState,
    pub mission_target: Option<Entity>,
    pub world_position: Vec2,
    pub heading: f32,
    pub waypoint: Vec2,
    pub fuel_current: f32,
    pub fuel_max: f32,
}

#[derive(Component, SSS)]
pub struct BuilderExpeditionDrone {
    pub home_base: Entity,
    pub save_data: Option<DroneSaveData>,
}
impl BuilderExpeditionDrone {
    pub fn new(home_base: Entity) -> Self {
        Self { home_base, save_data: None }
    }
    fn new_for_saving(drone: &ExpeditionDrone, drone_state: &DroneState, home_base: Entity, fuel: &DroneFuel, transform: &Transform, entity: Entity) -> Self {
        Self {
            home_base: home_base,
            save_data: Some(DroneSaveData {
                entity,
                state: *drone_state,
                mission_target: drone.mission_target,
                world_position: transform.translation.xy(),
                heading: drone.heading,
                waypoint: drone.waypoint,
                fuel_current: fuel.current,
                fuel_max: fuel.max,
            }),
        }
    }

    fn on_game_save(
        mut commands: Commands,
        drones: Query<(Entity, &ExpeditionDrone, &DroneState, &HomeBase, &DroneFuel, &Transform)>,
    ) {
        if drones.is_empty() { return; }
        let batch = drones.iter().map(|(entity, drone, drone_state, home_base, fuel, transform)| {
            BuilderExpeditionDrone::new_for_saving(drone, drone_state, home_base.0, fuel, transform, entity)
        }).collect::<SaveableBatchCommand<_>>();
        commands.queue(batch);
    }

    /// Spawns drone entity with visual components (beam, spot) when builder is added.
    /// - Creates drone sprite with fuel component
    /// - Spawns ScanSpot and ScanningBeam as separate entities linked by entity refs
    /// - Positions drone at home base (hidden) or saved world position
    fn on_add(
        trigger: On<Add, BuilderExpeditionDrone>,
        mut commands: Commands,
        builders: Query<&BuilderExpeditionDrone>,
        asset_server: Res<AssetServer>,
        home_bases: Query<&Transform, With<ExplorationCenter>>,
        mut meshes: ResMut<Assets<Mesh>>,
        mut beam_materials: ResMut<Assets<ScanningBeamMaterial>>,
        mut spot_materials: ResMut<Assets<ScanSpotMaterial>>,
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
                (DroneState::Stationed, None, home_pos, 0.0, Vec2::ZERO, DEFAULT_MAX_FUEL, DEFAULT_MAX_FUEL)
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
}

impl Saveable for BuilderExpeditionDrone {
    fn save(self, tx: &rusqlite::Transaction) -> rusqlite::Result<()> {
        let data = self.save_data.expect("BuilderExpeditionDrone for saving must have save_data");
        let entity_id = data.entity.index() as i64;
        let home_base_id = self.home_base.index() as i64;
        let mission_target_id = data.mission_target.map(|e| e.index() as i64);
        let state_u8: u8 = match data.state {
            DroneState::Stationed => 0,
            DroneState::Refueling => 1,
            DroneState::Deploying => 2,
            DroneState::Scanning => 3,
            DroneState::Returning => 4,
        };

        tx.register_entity(entity_id)?;
        tx.save_world_position(entity_id, data.world_position)?;
        tx.execute(
            "INSERT OR REPLACE INTO expedition_drones (id, home_base_id, state, mission_target_id, heading, waypoint_x, waypoint_y, fuel_current, fuel_max) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            rusqlite::params![entity_id, home_base_id, state_u8, mission_target_id, data.heading, data.waypoint.x, data.waypoint.y, data.fuel_current, data.fuel_max],
        )?;
        Ok(())
    }
}

impl Loadable for BuilderExpeditionDrone {
    fn load(ctx: &mut LoadContext) -> rusqlite::Result<LoadResult> {
        let mut stmt = ctx.conn.prepare("SELECT id, home_base_id, state, mission_target_id, heading, waypoint_x, waypoint_y, fuel_current, fuel_max FROM expedition_drones LIMIT ?1 OFFSET ?2")?;
        let mut rows = stmt.query(ctx.pagination.as_params())?;
        
        let mut count = 0;
        while let Some(row) = rows.next()? {
            let old_id: i64 = row.get(0)?;
            let home_base_old_id: i64 = row.get(1)?;
            let state_u8: u8 = row.get(2)?;
            let mission_target_old_id: Option<i64> = row.get(3)?;
            let heading: f32 = row.get(4)?;
            let waypoint_x: f32 = row.get(5)?;
            let waypoint_y: f32 = row.get(6)?;
            let fuel_current: f32 = row.get(7)?;
            let fuel_max: f32 = row.get(8)?;
            let world_position = ctx.conn.get_world_position(old_id)?;
            
            let Some(new_entity) = ctx.get_new_entity_for_old(old_id) else { continue; };
            let Some(new_home_base) = ctx.get_new_entity_for_old(home_base_old_id) else { continue; };
            let new_mission_target = mission_target_old_id.and_then(|id| ctx.get_new_entity_for_old(id));
            
            let state = match state_u8 {
                0 => DroneState::Stationed,
                1 => DroneState::Refueling,
                2 => DroneState::Deploying,
                3 => DroneState::Scanning,
                4 => DroneState::Returning,
                _ => DroneState::Stationed,
            };
            
            let save_data = DroneSaveData {
                entity: new_entity,
                state,
                mission_target: new_mission_target,
                world_position,
                heading,
                waypoint: Vec2::new(waypoint_x, waypoint_y),
                fuel_current,
                fuel_max,
            };
            let builder = BuilderExpeditionDrone {
                home_base: new_home_base,
                save_data: Some(save_data),
            };
            ctx.commands.entity(new_entity).insert(builder);
            count += 1;
        }
        Ok(count.into())
    }
}
