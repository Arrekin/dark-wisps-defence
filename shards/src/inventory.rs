use bevy::platform::collections::HashMap;
use bevy::prelude::*;

use game_core::prelude::ShardType;

#[derive(Resource, Default)]
pub struct ShardInventory {
    shards: HashMap<ShardType, usize>,
}
impl ShardInventory {
    pub fn count(&self, shard_type: ShardType) -> usize {
        self.shards.get(&shard_type).copied().unwrap_or(0)
    }

    pub fn has(&self, shard_type: ShardType) -> bool {
        self.count(shard_type) > 0
    }

    pub fn add(&mut self, shard_type: ShardType, amount: usize) {
        *self.shards.entry(shard_type).or_insert(0) += amount;
    }

    /// Removes one shard of the given type.
    ///
    /// Caller must verify `has(shard_type)` before calling. Panics if count is zero.
    pub fn remove(&mut self, shard_type: ShardType) {
        let count = self.shards.get_mut(&shard_type)
            .expect("ShardInventory::remove called for absent shard type");
        assert!(*count > 0, "ShardInventory::remove called with zero count");
        *count -= 1;
    }

    pub fn iter(&self) -> impl Iterator<Item = (ShardType, usize)> + '_ {
        self.shards.iter().map(|(&shard_type, &count)| (shard_type, count))
    }
}
