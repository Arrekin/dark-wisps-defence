use bevy::prelude::*;

use game_core::prelude::ShardType;

/// Announced when a shard blueprint is granted at runtime, so reactors (e.g. research obsolescence)
/// can respond without polling. Granters trigger this after calling `ShardBlueprints::unlock`.
#[derive(Event)]
pub struct ShardBlueprintAcquired(pub ShardType);

#[derive(Resource, Default)]
pub struct ShardBlueprints {
    unlocked: Vec<ShardType>,
}
impl ShardBlueprints {
    pub fn is_unlocked(&self, shard_type: ShardType) -> bool {
        self.unlocked.contains(&shard_type)
    }

    /// Grants a blueprint. Returns `true` if newly granted, `false` if already held — the lane owns
    /// its own dedup, so callers need not pre-check `is_unlocked`.
    pub fn unlock(&mut self, shard_type: ShardType) -> bool {
        if self.unlocked.contains(&shard_type) {
            return false;
        }
        self.unlocked.push(shard_type);
        true
    }

    /// Revokes a previously-granted blueprint. Blueprints are rights that can be taken away; a
    /// revoked blueprint is simply not saved (the saver writes current membership).
    pub fn revoke(&mut self, shard_type: ShardType) {
        self.unlocked.retain(|unlocked| *unlocked != shard_type);
    }

    pub fn iter(&self) -> impl Iterator<Item = ShardType> + '_ {
        self.unlocked.iter().copied()
    }
}
