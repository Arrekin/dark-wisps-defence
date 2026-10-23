use bevy::prelude::*;

use alteration::modifiers::prelude::AttackDamage;
use game_core::prelude::{SSS, ZDepth};

use super::components::Projectile;

#[derive(Component)]
#[require(AttackDamage, Projectile, ZDepth::Projectile)]
pub struct Rocket {
    /// Flight direction in radians.
    pub heading: f32,
}
#[derive(Component)]
#[require(ZDepth::ProjectileUnder)]
pub struct RocketExhaust;

/// Rocket follows Wisp, and if the wisp no longer exists, looks for another target
#[derive(Component)]
pub struct RocketTarget(pub Option<Entity>);

#[derive(Component, SSS)]
pub struct BuilderRocket {
    pub world_position: Vec2,
    pub heading: f32,
    pub target_wisp: Option<Entity>,
    pub damage: AttackDamage,
}
impl BuilderRocket {
    pub fn new(world_position: Vec2, heading: f32, damage: AttackDamage) -> Self {
        Self { world_position, heading, target_wisp: None, damage }
    }
    pub fn with_target_wisp(mut self, target_wisp: impl Into<Option<Entity>>) -> Self {
        self.target_wisp = target_wisp.into();
        self
    }
}
