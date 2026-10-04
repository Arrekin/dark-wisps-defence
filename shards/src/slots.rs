use bevy::prelude::*;

use game_core::prelude::Shard;

/// Fixed-capacity indexed shard slots attached to an entity.
///
/// Call `insert_at` to socket a shard; it writes the slot and triggers `ShardApplyEvent`
/// atomically. Callers without a specific target index should use `first_free_slot` first.
#[derive(Component)]
pub struct ShardSlots {
    slots: Vec<Option<Shard>>,
}
impl ShardSlots {
    pub fn new(capacity: usize) -> Self {
        Self { slots: vec![None; capacity] }
    }

    /// Sockets a shard into the given slot index, triggering `ShardApplyEvent`.
    /// Panics if `slot_index >= capacity`.
    pub fn insert_at(
        &mut self,
        slot_index: usize,
        shard: Shard,
        shard_target: Entity,
        commands: &mut Commands,
    ) {
        assert!(slot_index < self.slots.len(), "ShardSlots::insert_at: invalid slot index {slot_index}");
        self.slots[slot_index] = Some(shard);
        commands.trigger(ShardApplyEvent { shard_target, shard });
    }

    /// Returns the index of the first empty slot, or `None` if full.
    pub fn first_free_slot(&self) -> Option<usize> {
        self.slots.iter().position(|s| s.is_none())
    }

    pub fn capacity(&self) -> usize {
        self.slots.len()
    }

    pub fn len(&self) -> usize {
        self.slots.iter().filter(|s| s.is_some()).count()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn is_full(&self) -> bool {
        self.slots.iter().all(|s| s.is_some())
    }

    pub fn get(&self, slot_index: usize) -> Option<Shard> {
        self.slots.get(slot_index).and_then(|s| *s)
    }

    /// Iterates occupied slots as `(slot_index, Shard)`.
    pub fn iter_with_index(&self) -> impl Iterator<Item = (usize, Shard)> + '_ {
        self.slots.iter().enumerate().filter_map(|(i, s)| s.map(|t| (i, t)))
    }

    /// Iterates shards of occupied slots in slot-index order.
    pub fn iter(&self) -> impl Iterator<Item = Shard> + '_ {
        self.slots.iter().filter_map(|s| *s)
    }
}

/// Triggered by `ShardSlots::insert_at` when a shard is socketed.
///
/// Each entity type registers a per-entity observer for this event in its builder's `on_add`.
/// The observer is responsible for spawning the appropriate effect entity.
#[derive(EntityEvent, Clone, Copy)]
pub struct ShardApplyEvent {
    #[event_target]
    pub shard_target: Entity,
    pub shard: Shard,
}
