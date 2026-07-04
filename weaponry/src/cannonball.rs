use bevy::prelude::*;

use alteration::modifiers::prelude::AttackDamage;
use game_core::prelude::SSS;

use super::components::Projectile;

#[derive(Component)]
#[require(AttackDamage, Projectile)]
pub struct Cannonball;

// Cannonball follows Wisp, and if the wisp no longer exists, follows to the target position
#[derive(Component, Default)]
pub struct CannonballTarget{
    pub initial_distance: f32,
    pub target_position: Vec2,
}

#[derive(Component, SSS)]
pub struct BuilderCannonball {
    pub world_position: Vec2,
    pub target_position: Vec2,
    pub damage: AttackDamage,
    /// Original spawn distance. `new()` computes it from world/target positions;
    /// the loader overrides it via `with_initial_distance` to restore mid-flight
    /// state.
    pub initial_distance: f32,
}
impl BuilderCannonball {
    pub fn new(world_position: Vec2, target_position: Vec2, damage: AttackDamage) -> Self {
        Self {
            initial_distance: world_position.distance(target_position),
            world_position, target_position, damage,
        }
    }
    /// Override the initial distance (used by the loader to restore mid-flight
    /// cannonballs).
    pub fn with_initial_distance(mut self, initial_distance: f32) -> Self {
        self.initial_distance = initial_distance;
        self
    }
}
