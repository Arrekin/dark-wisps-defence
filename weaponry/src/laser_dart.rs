use bevy::prelude::*;

use alteration::modifiers::prelude::AttackDamage;
use game_core::prelude::{DrawLevel, SSS, ZDepth};

use super::components::{Projectile, ProjectileDirection};

pub const LASER_DART_SPEED: f32 = 600.0;

#[derive(Component)]
#[require(ZDepth::Projectile, DrawLevel::Airborne, AttackDamage, Projectile, ProjectileDirection)]
pub struct LaserDart;

#[derive(Component, SSS)]
pub struct BuilderLaserDart {
    pub world_position: Vec2,
    pub direction: Vec2,
    pub damage: AttackDamage,
}
impl BuilderLaserDart {
    pub fn new(world_position: Vec2, direction: Vec2, damage: AttackDamage) -> Self {
        Self { world_position, direction, damage }
    }

    /// A dart from `muzzle` aimed where a target keeping `target_velocity` meets it.
    ///
    /// Expects a target slower than `LASER_DART_SPEED`.
    pub fn aimed_at(muzzle: Vec2, target_position: Vec2, target_velocity: Vec2, damage: AttackDamage) -> Self {
        // The flight time t solves |offset + target_velocity * t| = LASER_DART_SPEED * t.
        let offset = target_position - muzzle;
        let closing = LASER_DART_SPEED * LASER_DART_SPEED - target_velocity.length_squared();
        let drift = offset.dot(target_velocity);
        let flight_time = (drift + (drift * drift + closing * offset.length_squared()).sqrt()) / closing;
        let meeting_point = target_position + target_velocity * flight_time;
        Self::new(muzzle, (meeting_point - muzzle).normalize(), damage)
    }
}
