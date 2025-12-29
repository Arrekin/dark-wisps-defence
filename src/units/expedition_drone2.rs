use bevy::{
    render::render_resource::AsBindGroup, 
    shader::ShaderRef,
    sprite_render::{AlphaMode2d, Material2d, Material2dPlugin}
};

use crate::prelude::*;

pub struct ExpeditionDrone2Plugin;
impl Plugin for ExpeditionDrone2Plugin {
    fn build(&self, app: &mut App) {
        app
            .add_plugins(Material2dPlugin::<ScanningRayMaterial>::default())
            .add_systems(OnEnter(GameState::Running), spawn_test_drone2)
            .add_systems(Update, (
                drone_movement_system.run_if(in_state(GameState::Running)),
                update_scan_spot_system.run_if(in_state(GameState::Running)).after(drone_movement_system),
                update_scanning_beam_system.run_if(in_state(GameState::Running)).after(update_scan_spot_system),
            ));
    }
}

pub const EXPEDITION_DRONE_BASE_IMAGE: &str = "units/expedition_drone.png";
const PATROL_RADIUS: f32 = 150.0;      // how far waypoints spawn from target center
const DRONE_SPEED: f32 = 80.0;          // world units per second
const DRONE_TURN_RATE: f32 = 1.2;       // radians per second (how fast it can turn)
const WAYPOINT_REACH_DIST: f32 = 2.0;  // how close before picking new waypoint
const DRONE_FRONT_OFFSET: f32 = 32.0; // pixels from drone center to front
const SCAN_ANGLE_LIMIT: f32 = 1.6;    // radians (~90°) - max angle from forward to scan
const SCAN_POINT_SPEED: f32 = 20.0; // world units per second - how fast beam moves to target
const SPOT_RADIUS: f32 = 25.0; // base radius of scan spot on ground
const SPOT_ELONGATION_FACTOR: f32 = 0.0015; // elongation per unit distance (0 = circle when close)
const BEAM_START_WIDTH: f32 = 2.0; // width at drone (narrow apex)

#[derive(Component)]
pub struct ExpeditionDrone2 {
    pub target_entity: Entity,   // entity with GridImprint we're scanning
    pub heading: f32,            // current facing direction in radians
    pub waypoint: Vec2,          // current waypoint we're flying toward
    pub turnaround_timer: f32,   // timer for turnaround waypoint stuck detection
    pub is_scanning: bool,       // true if target is within scan angle
}

#[derive(Component)]
pub struct ScanningRay {
    pub drone: Entity,
    pub spot: Entity,             // the scan spot entity
}

#[derive(Component)]
pub struct ScanSpot {
    pub current_pos: Vec2,        // current world position
    pub destination_pos: Vec2,    // where moving toward
}

// Material for the scanning ray effect
#[derive(Asset, TypePath, Debug, Clone, AsBindGroup)]
pub struct ScanningRayMaterial {
    // Width at the start (drone end) - normalized 0-1 over mesh width
    #[uniform(0)]
    pub start_width: f32,
    // Width at the end (target end) - normalized 0-1 over mesh width
    #[uniform(0)]
    pub end_width: f32,
    // Pulse animation value
    #[uniform(0)]
    pub pulse: f32,
    #[uniform(0)]
    pub is_spot: f32,  // 0.0 = beam, 1.0 = spot
}

impl Material2d for ScanningRayMaterial {
    fn fragment_shader() -> ShaderRef {
        "shaders/scanning_ray.wgsl".into()
    }
    fn alpha_mode(&self) -> AlphaMode2d {
        AlphaMode2d::Blend
    }
}

fn spawn_test_drone2(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    main_base: Query<(Entity, &Transform, &GridImprint), With<MainBase>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut ray_materials: ResMut<Assets<ScanningRayMaterial>>,
) {
    let Some((target_entity, target_transform, target_imprint)) = main_base.iter().next() else {
        return;
    };
    
    let center = target_transform.translation.xy();
    let target_size = target_imprint.world_size();
    
    // Start drone at edge of patrol area
    let mut rng = nanorand::tls_rng();
    let initial_angle: f32 = rng.generate::<f32>() * std::f32::consts::TAU;
    let drone_pos = center + Vec2::new(
        PATROL_RADIUS * initial_angle.cos(),
        PATROL_RADIUS * initial_angle.sin(),
    );
    
    // Initial heading: point toward center
    let initial_heading = (center - drone_pos).y.atan2((center - drone_pos).x);
    
    // First waypoint on opposite side
    let first_waypoint = center + Vec2::new(
        PATROL_RADIUS * (initial_angle + std::f32::consts::PI).cos(),
        PATROL_RADIUS * (initial_angle + std::f32::consts::PI).sin(),
    );
    
    // Initial random target point within the main base bounds
    let initial_target = random_point_in_bounds(&mut rng, center, target_size);
    
    // Beam mesh - just draws the cone lines, no circle
    let ray_mesh = meshes.add(Rectangle::new(1.0, 1.0));
    let ray_material = ray_materials.add(ScanningRayMaterial {
        start_width: 0.1,
        end_width: 0.4,
        pulse: 0.0,
        is_spot: 0.0,  // beam mode
    });
    
    // Spot mesh - circle at fixed world position
    let spot_mesh = meshes.add(Circle::new(1.0)); // Unit circle, scaled
    let spot_material = ray_materials.add(ScanningRayMaterial {
        start_width: 0.0, // unused for spot
        end_width: 0.0,
        pulse: 0.0,
        is_spot: 1.0,  // spot mode
    });
    
    // Spawn drone
    let drone_entity = commands.spawn((
        Sprite {
            image: asset_server.load(EXPEDITION_DRONE_BASE_IMAGE),
            ..default()
        },
        Transform {
            translation: drone_pos.extend(Z_AERIAL_UNIT),
            scale: Vec3::new(2.0, 2.0, 1.0),
            rotation: Quat::from_rotation_z(initial_heading),
        },
        ExpeditionDrone2 {
            target_entity,
            heading: initial_heading,
            waypoint: first_waypoint,
            turnaround_timer: 0.0,
            is_scanning: true,
        },
    )).id();
    
    // Spawn scan spot - fixed at target world position
    let spot_entity = commands.spawn((
        Mesh2d(spot_mesh),
        MeshMaterial2d(spot_material),
        Transform {
            translation: initial_target.extend(Z_GROUND_EFFECT),
            scale: Vec3::splat(SPOT_RADIUS), // Matches SPOT_RADIUS constant
            ..default()
        },
        ScanSpot {
            current_pos: initial_target,
            destination_pos: initial_target,
        },
    )).id();
    
    // Spawn beam - connects drone to spot
    commands.spawn((
        Mesh2d(ray_mesh),
        MeshMaterial2d(ray_material),
        Transform::from_translation(Vec3::new(0.0, 0.0, Z_GROUND_EFFECT)),
        ScanningRay {
            drone: drone_entity,
            spot: spot_entity,
        },
    ));
}

fn random_point_in_bounds(rng: &mut nanorand::tls::TlsWyRand, center: Vec2, size: Vec2) -> Vec2 {
    Vec2::new(
        center.x + (rng.generate::<f32>() - 0.5) * size.x,
        center.y + (rng.generate::<f32>() - 0.5) * size.y,
    )
}

fn drone_movement_system(
    time: Res<Time>,
    targets: Query<&Transform, Without<ExpeditionDrone2>>,
    mut drones: Query<(&mut Transform, &mut ExpeditionDrone2)>,
) {
    let mut rng = nanorand::tls_rng();
    
    for (mut transform, mut drone) in drones.iter_mut() {
        // Get target center position
        let Ok(target_transform) = targets.get(drone.target_entity) else { continue; };
        let center = target_transform.translation.xy();
        let drone_pos = transform.translation.xy();
        
        // Check angle to target center
        let to_target = center - drone_pos;
        let angle_to_target = to_target.y.atan2(to_target.x);
        let angle_diff = (angle_to_target - drone.heading + std::f32::consts::PI)
            .rem_euclid(std::f32::consts::TAU) - std::f32::consts::PI;
        let target_in_front = angle_diff.abs() < SCAN_ANGLE_LIMIT;
        
        // Determine desired heading based on whether target is in front
        let desired_heading = if target_in_front {
            // Target is in front - aim directly at target center
            angle_to_target
        } else {
            // Target is behind - we've overshot, head toward turnaround waypoint
            // Pick new waypoint if we don't have one or reached it
            let to_waypoint = drone.waypoint - drone_pos;
            let dist_to_waypoint = to_waypoint.length();
            
            drone.turnaround_timer += time.delta_secs();
            let is_stuck = drone.turnaround_timer > 5.0;
            
            if dist_to_waypoint < WAYPOINT_REACH_DIST || is_stuck {
                // Reset timer and pick new turnaround waypoint
                drone.turnaround_timer = 0.0;
                
                // Pick waypoint that will bring us back toward target
                // Go to a point past the target in our current direction of travel
                let overshoot_angle = drone.heading + std::f32::consts::PI;
                let angle_variation = (rng.generate::<f32>() - 0.5) * std::f32::consts::FRAC_PI_4;
                let waypoint_angle = overshoot_angle + angle_variation;
                
                drone.waypoint = center + Vec2::new(
                    PATROL_RADIUS * waypoint_angle.cos(),
                    PATROL_RADIUS * waypoint_angle.sin(),
                );
            }
            
            // Head toward waypoint
            to_waypoint.y.atan2(to_waypoint.x)
        };
        
        // Smoothly turn toward desired heading (limited turn rate)
        let heading_diff = (desired_heading - drone.heading + std::f32::consts::PI)
            .rem_euclid(std::f32::consts::TAU) - std::f32::consts::PI;
        let max_turn = DRONE_TURN_RATE * time.delta_secs();
        let turn_amount = heading_diff.clamp(-max_turn, max_turn);
        drone.heading += turn_amount;
        
        // Move forward in current heading direction
        let forward = Vec2::new(drone.heading.cos(), drone.heading.sin());
        let new_pos = drone_pos + forward * DRONE_SPEED * time.delta_secs();
        transform.translation.x = new_pos.x;
        transform.translation.y = new_pos.y;
        
        // Update rotation to match heading
        transform.rotation = Quat::from_rotation_z(drone.heading);
        
        // Update scanning state
        drone.is_scanning = target_in_front;
    }
}

fn update_scan_spot_system(
    time: Res<Time>,
    drones: Query<(&Transform, &ExpeditionDrone2), Without<ScanSpot>>,
    targets: Query<(&Transform, &GridImprint), (Without<ExpeditionDrone2>, Without<ScanSpot>, Without<ScanningRay>)>,
    beams: Query<&ScanningRay>,
    mut spots: Query<(&mut Transform, &mut ScanSpot, &MeshMaterial2d<ScanningRayMaterial>), Without<ExpeditionDrone2>>,
    mut ray_materials: ResMut<Assets<ScanningRayMaterial>>,
) {
    let mut rng = nanorand::tls_rng();
    
    for beam in beams.iter() {
        let Ok((mut spot_transform, mut spot, material_handle)) = spots.get_mut(beam.spot) else { continue; };
        let Ok((drone_transform, drone)) = drones.get(beam.drone) else { continue; };
        let Ok((target_transform, target_imprint)) = targets.get(drone.target_entity) else { continue; };
        
        let target_center = target_transform.translation.xy();
        let target_size = target_imprint.world_size();
        let drone_pos = drone_transform.translation.xy();
        
        // Only move spot when drone is actively scanning
        if drone.is_scanning {
            // Move spot toward destination
            let to_destination = spot.destination_pos - spot.current_pos;
            let distance_to_dest = to_destination.length();
            
            if distance_to_dest < 2.0 {
                // Reached destination, pick a new random point
                spot.destination_pos = random_point_in_bounds(&mut rng, target_center, target_size);
            } else {
                // Move toward destination
                let move_amount = SCAN_POINT_SPEED * time.delta_secs();
                if move_amount >= distance_to_dest {
                    spot.current_pos = spot.destination_pos;
                } else {
                    spot.current_pos += to_destination.normalize() * move_amount;
                }
            }
            
            // Update spot position
            spot_transform.translation.x = spot.current_pos.x;
            spot_transform.translation.y = spot.current_pos.y;
            
            // OVAL PROJECTION: Rotate spot to point toward drone
            let to_drone = drone_pos - spot.current_pos;
            let distance = to_drone.length();
            let angle_to_drone = to_drone.y.atan2(to_drone.x);
            spot_transform.rotation = Quat::from_rotation_z(angle_to_drone);
            // Elongation: 1.0 (circle) when close, stretches more as drone gets farther
            let elongation = 1.0 + distance * SPOT_ELONGATION_FACTOR;
            spot_transform.scale = Vec3::new(SPOT_RADIUS * elongation, SPOT_RADIUS, 1.0);
            // Pulse animation for spot (only when visible)
            if let Some(material) = ray_materials.get_mut(material_handle) {
                material.pulse = (material.pulse + time.delta_secs() * 1.2) % 1.0;
            }
        } else {
            // Hide spot when not scanning (scale to zero)
            spot_transform.scale = Vec3::ZERO;
        }
    }
}

fn update_scanning_beam_system(
    time: Res<Time>,
    drones: Query<(&Transform, &ExpeditionDrone2)>,
    spots: Query<&ScanSpot>,
    mut beams: Query<(&mut Transform, &ScanningRay, &MeshMaterial2d<ScanningRayMaterial>), (Without<ExpeditionDrone2>, Without<ScanSpot>)>,
    mut ray_materials: ResMut<Assets<ScanningRayMaterial>>,
) {
    for (mut beam_transform, beam, material_handle) in beams.iter_mut() {
        let Ok((drone_transform, drone)) = drones.get(beam.drone) else { continue; };
        let Ok(spot) = spots.get(beam.spot) else { continue; };
        
        // Hide beam when not scanning
        if !drone.is_scanning {
            beam_transform.scale = Vec3::ZERO;
            continue;
        }
        
        // Get drone position and facing direction
        let drone_pos = drone_transform.translation.xy();
        let drone_forward = (drone_transform.rotation * Vec3::X).xy().normalize();
        
        // Calculate drone front position (where beam starts)
        let beam_start = drone_pos + drone_forward * DRONE_FRONT_OFFSET;
        
        // Target is the spot's current position
        let target_point = spot.current_pos;
        
        // Calculate beam geometry
        let beam_vec = target_point - beam_start;
        let beam_length = beam_vec.length();
        let beam_angle = beam_vec.y.atan2(beam_vec.x);
        
        // Position beam: center it between start and end
        let beam_center = (beam_start + target_point) / 2.0;
        beam_transform.translation = beam_center.extend(Z_GROUND_EFFECT);
        
        // Rotate beam to point from drone to target
        beam_transform.rotation = Quat::from_rotation_z(beam_angle);
        
        // Scale beam: X is length, Y is spot diameter (so beam connects to spot edges)
        let spot_diameter = SPOT_RADIUS * 2.0;
        beam_transform.scale = Vec3::new(beam_length, spot_diameter, 1.0);
        
        // Update material - start_width is narrow apex, end_width matches spot
        if let Some(material) = ray_materials.get_mut(material_handle) {
            material.start_width = BEAM_START_WIDTH / spot_diameter; // Small apex
            material.end_width = 1.0; // Full width at spot end
            material.pulse = (material.pulse + time.delta_secs() * 0.8) % 1.0;
        }
    }
}
