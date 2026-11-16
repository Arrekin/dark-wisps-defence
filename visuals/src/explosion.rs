use bevy::prelude::*;

use game_core::prelude::GridPosition;

/// Spawns a one-cell explosion sprite drawn at the position's level.
#[derive(Component)]
pub struct BuilderExplosion(pub GridPosition);
