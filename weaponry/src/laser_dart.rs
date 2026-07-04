use bevy::prelude::*;

use alteration::modifiers::prelude::AttackDamage;
use game_core::prelude::SSS;

use super::components::Projectile;

#[derive(Component)]
pub struct LaserDart;

// LaserDart follows Wisp, and if the wisp no longer exists, follows to the target vector
#[derive(Component, Default)]
#[require(AttackDamage, Projectile)]
pub struct LaserDartTarget {
    pub target_wisp: Option<Entity>,
    pub target_vector: Vec2,
}

#[derive(Component, SSS)]
pub struct BuilderLaserDart {
    pub world_position: Vec2,
    pub target_wisp: Option<Entity>,
    pub target_vector: Vec2,
    pub damage: AttackDamage,
}
impl BuilderLaserDart {
    pub fn new(world_position: Vec2, target_wisp: Entity, target_vector: Vec2, damage: AttackDamage) -> Self {
        Self { world_position, target_wisp: Some(target_wisp), target_vector, damage }
    }
    /// Override the target wisp. Used by the loader when the saved target may
    /// have been despawned (mapped to `None`).
    pub fn with_target_wisp(mut self, target_wisp: Option<Entity>) -> Self {
        self.target_wisp = target_wisp;
        self
    }
}
