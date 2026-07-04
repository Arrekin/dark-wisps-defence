use bevy::prelude::*;

use game_core::prelude::{MapBound, SSS};

#[derive(Component, SSS)]
pub struct BuilderRipple {
    pub world_position: Vec2,
    pub radius: f32, // in world size
    /// Current expansion radius. `new()` defaults to 0.0 (fresh spawn); the
    /// loader overrides it via `with_current_radius` to restore mid-expansion
    /// state.
    pub current_radius: f32,
}
impl BuilderRipple {
    pub fn new(world_position: Vec2, radius: f32) -> Self {
        Self { world_position, radius, current_radius: 0.0 }
    }
    /// Override the current radius (used by the loader to restore mid-expansion
    /// ripples).
    pub fn with_current_radius(mut self, current_radius: f32) -> Self {
        self.current_radius = current_radius;
        self
    }
}

#[derive(Component)]
#[require(MapBound)]
pub struct Ripple {
    pub max_radius: f32,
    pub current_radius: f32,
}
impl Ripple {
    /// Current radius as a fraction of the full diameter, range 0..0.5.
    /// Matches the normalised radius the shader uses internally.
    pub fn normalized_radius(&self) -> f32 {
        self.current_radius / (self.max_radius * 2.0)
    }

    pub fn max_radius(&self) -> f32 {
        self.max_radius
    }
}
