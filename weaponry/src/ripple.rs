use bevy::prelude::*;

use game_core::prelude::{MapBound, SSS, ZDepth};

#[derive(Component, SSS)]
pub struct BuilderRipple {
    pub world_position: Vec2,
    /// Full radius, in world units.
    pub max_radius: f32,
    /// Current expansion radius; 0 on a fresh spawn.
    pub current_radius: f32,
}
impl BuilderRipple {
    pub fn new(world_position: Vec2, max_radius: f32) -> Self {
        Self { world_position, max_radius, current_radius: 0.0 }
    }
    pub fn with_current_radius(mut self, current_radius: f32) -> Self {
        self.current_radius = current_radius;
        self
    }
}

#[derive(Component)]
#[require(MapBound, ZDepth::AirEffect)]
pub struct Ripple {
    pub max_radius: f32,
    pub current_radius: f32,
}
impl Ripple {
    /// Current radius as a fraction of the full diameter, range 0..0.5.
    pub fn normalized_radius(&self) -> f32 {
        self.current_radius / (self.max_radius * 2.0)
    }
}
