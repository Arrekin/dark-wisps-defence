use bevy::prelude::*;

use alteration::modifiers::prelude::AttackDamage;
use game_core::prelude::{SSS, ZDepth};

use super::{components::Projectile, lob::Lob};

#[derive(Component)]
#[require(AttackDamage, Projectile, ZDepth::Projectile)]
pub struct Fireball;

#[derive(Component, SSS)]
pub struct BuilderFireball {
    pub lob: Lob,
    pub damage: AttackDamage,
    /// The entity that fired it, if known.
    pub shooter: Option<Entity>,
}
impl BuilderFireball {
    pub fn new(lob: Lob, damage: AttackDamage) -> Self {
        Self { lob, damage, shooter: None }
    }
    pub fn with_shooter(mut self, shooter: Entity) -> Self {
        self.shooter = Some(shooter);
        self
    }
}
