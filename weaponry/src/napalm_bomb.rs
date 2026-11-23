use bevy::prelude::*;

use game_core::prelude::{DrawLevel, SSS, ZDepth};

use super::{components::{Projectile, ProjectileLaunchCounter}, lob::{LobArc, LobArcProgress}};

/// The Napalm Battery's lobbed projectile. Hidden while it awaits launch.
#[derive(Component)]
#[require(Projectile, ZDepth::Projectile, DrawLevel::Airborne)]
pub struct NapalmBomb;

#[derive(Component, SSS)]
pub struct BuilderNapalmBomb {
    pub arc: LobArc,
    /// The entity that fired it, if known.
    pub shooter: Option<Entity>,
    /// Deferred launch time, or `None` to launch immediately on spawn.
    pub launch_counter: Option<ProjectileLaunchCounter>,
    /// Saved flight progress. 0 on fresh spawn.
    pub progress: LobArcProgress,
}
impl BuilderNapalmBomb {
    pub fn new(arc: LobArc) -> Self {
        Self { arc, shooter: None, launch_counter: None, progress: LobArcProgress::default() }
    }
    pub fn with_shooter(mut self, shooter: Entity) -> Self {
        self.shooter = Some(shooter);
        self
    }
    pub fn with_launch_counter(mut self, launch_counter: impl Into<Option<ProjectileLaunchCounter>>) -> Self {
        self.launch_counter = launch_counter.into();
        self
    }
    pub fn with_progress(mut self, progress: LobArcProgress) -> Self {
        self.progress = progress;
        self
    }
}
