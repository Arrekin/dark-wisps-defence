use bevy::prelude::*;

use alteration::modifiers::prelude::AttackDamage;
use game_core::prelude::{SSS, ZDepth};

use super::components::Projectile;

#[derive(Component)]
#[require(AttackDamage, Projectile, ZDepth::PROJECTILE)]
pub struct Rocket;
#[derive(Component)]
#[require(ZDepth::PROJECTILE_UNDER)]
pub struct RocketExhaust;

// Rocket follows Wisp, and if the wisp no longer exists, looks for another target
#[derive(Component)]
pub struct RocketTarget(pub Entity);

#[derive(Component, SSS)]
pub struct BuilderRocket {
    pub world_position: Vec2,
    pub rotation: Quat,
    pub target_wisp: Entity,
    pub damage: AttackDamage,
}
impl BuilderRocket {
    pub fn new(world_position: Vec2, rotation: Quat, target_wisp: Entity, damage: AttackDamage) -> Self {
        Self { world_position, rotation, target_wisp, damage }
    }
}
