use bevy::prelude::*;

use game_core::prelude::{MapBound, SSS};
use persistence::{prelude::{GameDbHelpers, Loadable, LoadContext, LoadResult, Saveable}, rusqlite};

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
#[require(MapBound)]
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
#[require(MapBound)]
pub struct ScanningBeam {
    pub drone: Entity,
    pub spot: Entity,  // the scan spot entity
}

#[derive(Component)]
#[require(MapBound)]
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
    pub fn new_for_saving(drone: &ExpeditionDrone, drone_state: &DroneState, home_base: Entity, fuel: &DroneFuel, transform: &Transform, entity: Entity) -> Self {
        Self {
            home_base,
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
}

impl Saveable for BuilderExpeditionDrone {
    fn save(self, tx: &rusqlite::Transaction) -> rusqlite::Result<()> {
        let data = self.save_data.expect("BuilderExpeditionDrone for saving must have save_data");
        let entity_id = data.entity.index_u32() as i64;
        let home_base_id = self.home_base.index_u32() as i64;
        let mission_target_id = data.mission_target.map(|e| e.index_u32() as i64);
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
