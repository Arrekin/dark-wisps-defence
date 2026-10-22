use bevy::prelude::*;
use game_core::prelude::MapBound;

#[derive(Component, Default)]
#[require(MapBound)]
pub struct Projectile;

/// The entity that fired this projectile.
#[derive(Component)]
#[relationship(relationship_target = FiredProjectiles)]
pub struct ProjectileOf(pub Entity);

/// Projectiles this entity fired that still exist, in firing order.
#[derive(Component, Default)]
#[relationship_target(relationship = ProjectileOf)]
pub struct FiredProjectiles(Vec<Entity>);
impl FiredProjectiles {
    /// The most recently fired projectile.
    pub fn latest(&self) -> Option<Entity> {
        self.0.last().copied()
    }

    /// The fired projectiles, most recent first.
    pub fn newest_first(&self) -> impl Iterator<Item = Entity> + '_ {
        self.0.iter().rev().copied()
    }
}
