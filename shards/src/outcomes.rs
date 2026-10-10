use bevy::prelude::*;

use game_core::prelude::ShardType;

/// Outcome that unlocks a shard type in `ShardBlueprints`.
/// Triggers `ShardBlueprintAcquired` only if the blueprint is newly unlocked.
#[derive(Component, Clone, Copy, Debug, Default)]
#[component(immutable)]
pub struct UnlockShardBlueprint(pub ShardType);
