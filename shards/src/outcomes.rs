use bevy::prelude::*;

use game_core::prelude::ShardType;

/// Outcome kind: unlock a shard blueprint on the player's shard inventory.
/// Self-observed by `shards_internal::ShardOutcomesPlugin`; on `FulfillOutcome`
/// it unlocks the blueprint via the `ShardBlueprints` resource and announces
/// `ShardBlueprintAcquired` so reactors can respond without polling.
#[derive(Component, Clone, Copy, Debug, Default)]
#[component(immutable)]
pub struct UnlockShardBlueprint(pub ShardType);
