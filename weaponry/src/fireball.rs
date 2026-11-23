use bevy::prelude::*;

use alteration::modifiers::prelude::AttackDamage;
use game_core::prelude::{DrawLevel, SSS, ZDepth};

use super::{components::Projectile, lob::{LobArc, LobArcProgress}};

#[derive(Component)]
#[require(AttackDamage, Projectile, ZDepth::Projectile, DrawLevel::Airborne)]
pub struct Fireball;

#[derive(Component, SSS)]
pub struct BuilderFireball {
    pub arc: LobArc,
    pub damage: AttackDamage,
    /// The entity that fired it, if known.
    pub shooter: Option<Entity>,
    /// Saved flight progress. 0 on fresh spawn.
    pub progress: LobArcProgress,
}
impl BuilderFireball {
    pub fn new(arc: LobArc, damage: AttackDamage) -> Self {
        Self { arc, damage, shooter: None, progress: LobArcProgress::default() }
    }
    pub fn with_shooter(mut self, shooter: Entity) -> Self {
        self.shooter = Some(shooter);
        self
    }
    pub fn with_progress(mut self, progress: LobArcProgress) -> Self {
        self.progress = progress;
        self
    }
}
