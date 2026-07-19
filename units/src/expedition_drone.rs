use bevy::prelude::*;

use game_core::prelude::{MapBound, SSS, ZDepth};

/// Drone cost in dark ore - kept as constant for easy balancing
pub const DRONE_COST_ORE: u32 = 100;

// Movement tuning
pub const PATROL_RADIUS: f32 = 150.0;
pub const DRONE_SPEED: f32 = 160.0;
pub const DRONE_TURN_RATE: f32 = 1.2;           // radians/sec - gives smooth arcing flight
pub const WAYPOINT_REACH_DIST: f32 = 2.0;

// Scanning geometry
pub const DRONE_FRONT_OFFSET: f32 = 32.0;       // beam origin offset from sprite center
pub const SCAN_ANGLE_LIMIT: f32 = 1.6;          // ~90° cone in front of drone
pub const SCAN_POINT_SPEED: f32 = 20.0;
pub const SPOT_RADIUS: f32 = 25.0;
pub const SPOT_ELONGATION_FACTOR: f32 = 0.0015; // perspective effect: spot stretches with distance
pub const BEAM_START_WIDTH: f32 = 2.0;

// Fuel balance (fuel ≈ seconds of active flight at 1:1 ratio with FUEL_CONSUMPTION_RATE)
const DEFAULT_MAX_FUEL: f32 = 60.0;
pub const FUEL_CONSUMPTION_RATE: f32 = 3.0;
pub const REFUEL_RATE: f32 = 15.0;              // ~4 seconds to full refuel
pub const SCAN_PROGRESS_RATE: f32 = 100.;

pub const EXPEDITION_DRONE_BASE_IMAGE: &str = "units/expedition_drone.png";

/// Request to deploy a drone to a specific target
#[derive(Event)]
pub struct ExpeditionDroneDeploymentRequest {
    pub drone: Entity,
    pub target: Entity,
}

/// Event to recall a drone back to base
#[derive(EntityEvent)]
pub struct RecallDrone(pub Entity);

/// Drone state machine. Immutable component - state changes trigger `on_state_changed_handle_drone_state_change` observer.
/// See module docs for state transition diagram.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq, strum::Display)]
#[component(immutable)]
pub enum DroneState {
    #[default]
    Stationed,
    Refueling,
    Deploying,
    Scanning,
    Returning,
}

#[derive(Component)]
#[require(MapBound, ZDepth::AERIAL_UNIT)]
pub struct ExpeditionDrone {
    pub mission_target: Option<Entity>, // current mission target (ExpeditionZone)
    pub heading: f32,             // current facing direction in radians
    pub waypoint: Vec2,           // current waypoint we're flying toward
    pub is_beam_active: bool,     // true when target is in front and beam should show
}
impl ExpeditionDrone {
    /// Generate waypoint roughly opposite current heading with random variation.
    /// This creates natural figure-8 patrol patterns around the target.
    pub fn set_new_waypoint(&mut self, target_center: Vec2, rng: &mut nanorand::tls::TlsWyRand) {
        use nanorand::Rng;
        let overshoot_angle = self.heading + std::f32::consts::PI;
        let angle_variation = (rng.generate::<f32>() - 0.5) * std::f32::consts::FRAC_PI_4;
        let waypoint_angle = overshoot_angle + angle_variation;
        self.waypoint = target_center + Vec2::new(
            PATROL_RADIUS * waypoint_angle.cos(),
            PATROL_RADIUS * waypoint_angle.sin(),
        );
    }
}

#[derive(Component)]
#[require(MapBound, ZDepth::GROUND_EFFECT)]
pub struct ScanningBeam {
    pub drone: Entity,
    pub spot: Entity,  // the scan spot entity
}

#[derive(Component)]
#[require(MapBound, ZDepth::GROUND_EFFECT)]
pub struct ScanSpot {
    pub destination: Vec2, // where moving toward (world coords)
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

    /// Fuel cost for one-way travel. Used by UI to show deployment cost.
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

////////////////////////////////////////////
//              Builder
////////////////////////////////////////////

#[derive(Component, SSS)]
pub struct BuilderExpeditionDrone {
    pub home_base: Entity,
    pub state: DroneState,
    pub mission_target: Option<Entity>,
    /// Saved world position. `None` ⇒ compute from home base (fresh spawn);
    /// `Some` ⇒ override with saved value (restore).
    pub world_position: Option<Vec2>,
    pub heading: f32,
    pub waypoint: Vec2,
    pub fuel_current: f32,
    pub fuel_max: f32,
}
impl BuilderExpeditionDrone {
    pub fn new(home_base: Entity) -> Self {
        Self {
            home_base,
            state: DroneState::Stationed,
            mission_target: None,
            world_position: None,
            heading: 0.0,
            waypoint: Vec2::ZERO,
            fuel_current: DEFAULT_MAX_FUEL,
            fuel_max: DEFAULT_MAX_FUEL,
        }
    }
    pub fn with_state(mut self, state: DroneState) -> Self {
        self.state = state;
        self
    }
    pub fn with_mission_target(mut self, mission_target: Entity) -> Self {
        self.mission_target = Some(mission_target);
        self
    }
    pub fn with_world_position(mut self, world_position: Vec2) -> Self {
        self.world_position = Some(world_position);
        self
    }
    pub fn with_heading(mut self, heading: f32) -> Self {
        self.heading = heading;
        self
    }
    pub fn with_waypoint(mut self, waypoint: Vec2) -> Self {
        self.waypoint = waypoint;
        self
    }
    pub fn with_fuel(mut self, current: f32, max: f32) -> Self {
        self.fuel_current = current;
        self.fuel_max = max;
        self
    }
}
