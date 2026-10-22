use bevy::prelude::*;

use game_core::prelude::{SSS, ZDepth};

use super::{components::Projectile, lob::Lob};

/// The Napalm Battery's lobbed projectile.
#[derive(Component)]
#[require(Projectile, ZDepth::Projectile)]
pub struct NapalmBomb;

#[derive(Component, SSS)]
pub struct BuilderNapalmBomb {
    pub lob: Lob,
    /// The entity that fired it, if known.
    pub shooter: Option<Entity>,
}
impl BuilderNapalmBomb {
    pub fn new(lob: Lob) -> Self {
        Self { lob, shooter: None }
    }
    pub fn with_shooter(mut self, shooter: Entity) -> Self {
        self.shooter = Some(shooter);
        self
    }
}
