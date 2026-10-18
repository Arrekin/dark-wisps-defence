use bevy::prelude::*;

use game_core::prelude::MapBound;

/// Fire dome spawned when a `TowerInferno` discharges, centered at its `Transform`.
#[derive(Component)]
#[require(MapBound, Transform, Visibility)]
pub struct InfernoDome {
    pub radius: f32,
    /// Wrapped game time of the discharge, from `GameClock::wrapped`.
    pub start_time: f32,
}
impl InfernoDome {
    pub fn new(radius: f32, start_time: f32) -> Self {
        Self { radius, start_time }
    }
}
