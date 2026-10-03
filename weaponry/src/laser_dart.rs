use bevy::prelude::*;

use alteration::modifiers::prelude::AttackDamage;
use game_core::prelude::{SSS, ZDepth};

use super::components::Projectile;

#[derive(Component)]
#[require(ZDepth::PROJECTILE)]
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
    pub fn new(world_position: Vec2, target_vector: Vec2, damage: AttackDamage) -> Self {
        Self { world_position, target_wisp: None, target_vector, damage }
    }
    pub fn with_target_wisp(mut self, target_wisp: impl Into<Option<Entity>>) -> Self {
        self.target_wisp = target_wisp.into();
        self
    }
}
