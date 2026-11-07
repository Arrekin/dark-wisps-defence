use bevy::prelude::*;

use game_core::prelude::GridVolumePosition;

/// Spawns a one-cell explosion sprite drawn at the position's volume.
#[derive(Component)]
pub struct BuilderExplosion(pub GridVolumePosition);
