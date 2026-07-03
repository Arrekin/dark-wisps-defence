use bevy::prelude::*;
use game_core::prelude::GridCoords;

#[derive(Component)]
pub struct BuilderExplosion(pub GridCoords);
