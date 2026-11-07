use bevy::prelude::*;

use game_core::prelude::VolumeLevel;

/// Spawns a wisp attack effect drawn at the level of what the wisp hit.
#[derive(Component)]
pub struct BuilderWispAttackEffect {
    pub world_position: Vec2,
    pub level: VolumeLevel,
}
