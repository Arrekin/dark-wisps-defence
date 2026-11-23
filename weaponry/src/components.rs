use bevy::prelude::*;
use game_core::prelude::MapBound;

#[derive(Component, Default)]
#[require(MapBound, Transform, Visibility)]
pub struct Projectile;

/// Unit vector along which a projectile flies straight.
#[derive(Component, Clone, Copy, Default)]
pub struct ProjectileDirection(pub Vec2);

/// Delays a projectile's launch until the game clock reaches `launch_at` (in seconds).
#[derive(Component, Clone, Copy, Debug)]
pub struct ProjectileLaunchCounter {
    pub launch_at: f64,
}
impl ProjectileLaunchCounter {
    pub fn new(launch_at: f64) -> Self {
        Self { launch_at }
    }
}

/// Triggered on a projectile when the game clock reaches its launch time and
/// [`ProjectileLaunchCounter`] is removed.
#[derive(EntityEvent)]
pub struct ProjectileLaunched(pub Entity);

/// The entity that fired this projectile.
#[derive(Component)]
#[relationship(relationship_target = FiredProjectiles)]
pub struct ProjectileOf(pub Entity);

/// Live projectiles fired by this entity, in spawn order.
#[derive(Component, Default)]
#[relationship_target(relationship = ProjectileOf)]
pub struct FiredProjectiles(Vec<Entity>);
impl FiredProjectiles {
    /// The most recently fired projectile.
    pub fn latest(&self) -> Option<Entity> {
        self.0.last().copied()
    }
}
