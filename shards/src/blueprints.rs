use std::collections::BTreeSet;

use bevy::prelude::*;

use game_core::prelude::ShardType;

/// Announces a newly granted blueprint. Callers trigger this after `ShardBlueprints::unlock` succeeds.
#[derive(Event)]
pub struct ShardBlueprintAcquired(pub ShardType);

#[derive(Resource, Default, Clone)]
pub struct ShardBlueprints {
    unlocked: BTreeSet<ShardType>,
}
impl ShardBlueprints {
    pub fn is_unlocked(&self, shard_type: ShardType) -> bool {
        self.unlocked.contains(&shard_type)
    }

    /// Grants a blueprint. Returns `true` if newly granted.
    pub fn unlock(&mut self, shard_type: ShardType) -> bool {
        self.unlocked.insert(shard_type)
    }

    /// Revokes a granted blueprint.
    pub fn revoke(&mut self, shard_type: ShardType) {
        self.unlocked.remove(&shard_type);
    }

    /// Unlocked shard types, in `ShardType` order.
    pub fn iter(&self) -> impl Iterator<Item = ShardType> {
        self.unlocked.iter().copied()
    }
}
