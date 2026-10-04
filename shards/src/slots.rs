use bevy::prelude::*;

use game_core::prelude::Shard;

/// The shards socketed into an entity: slot `i` holds what is socketed into stat socket `i` of its
/// building. The slot count is fixed at spawn to the building's socket count.
///
/// Trigger `ShardSocketOperation` to change what is socketed. Its observer keeps each slot's shard
/// and the effect it applies in step.
#[derive(Component)]
pub struct ShardSlots(Box<[Option<SocketedShard>]>);
impl ShardSlots {
    pub fn new(socket_count: usize) -> Self {
        Self(vec![None; socket_count].into_boxed_slice())
    }

    /// The shard in a slot; `None` when empty.
    pub fn shard(&self, slot_index: usize) -> Option<Shard> {
        self.0.get(slot_index).copied().flatten().map(|socketed| socketed.shard)
    }

    /// Every slot's shard in slot-index order, `None` for empty ones.
    pub fn iter(&self) -> impl Iterator<Item = Option<Shard>> {
        self.0.iter().map(|slot| slot.map(|socketed| socketed.shard))
    }

    /// Puts a shard into a slot, returning what it replaced. Panics if `slot_index` has no slot.
    pub fn socket(&mut self, slot_index: usize, socketed: SocketedShard) -> Option<SocketedShard> {
        self.0[slot_index].replace(socketed)
    }

    /// Empties a slot, returning what it held.
    pub fn unsocket(&mut self, slot_index: usize) -> Option<SocketedShard> {
        self.0.get_mut(slot_index).and_then(Option::take)
    }
}

/// A shard held in a slot, with the `ShardEffect` entity it spawned. The effect is not saved;
/// loading sockets the shard again, which spawns a new one.
#[derive(Clone, Copy, Debug)]
pub struct SocketedShard {
    pub shard: Shard,
    pub effect: Entity,
}

/// Changes what a slot holds:
/// - `socket` takes the shard out of `Stock` and puts it into the slot. Does nothing if the socket
///   does not accept the shard or `Stock` does not have it.
/// - `restore` puts a saved shard into the slot without touching `Stock`. Does nothing if the socket
///   does not accept the shard.
/// - `unsocket` empties the slot.
///
/// Any shard the slot already held goes back to `Stock`.
#[derive(EntityEvent, Clone, Copy)]
pub struct ShardSocketOperation {
    #[event_target]
    pub shard_target: Entity,
    pub slot_index: usize,
    pub kind: ShardSocketOperationKind,
}
impl ShardSocketOperation {
    pub fn socket(shard_target: Entity, slot_index: usize, shard: Shard) -> Self {
        Self { shard_target, slot_index, kind: ShardSocketOperationKind::Socket(shard) }
    }

    pub fn restore(shard_target: Entity, slot_index: usize, shard: Shard) -> Self {
        Self { shard_target, slot_index, kind: ShardSocketOperationKind::Restore(shard) }
    }

    pub fn unsocket(shard_target: Entity, slot_index: usize) -> Self {
        Self { shard_target, slot_index, kind: ShardSocketOperationKind::Unsocket }
    }
}

#[derive(Clone, Copy, Debug)]
pub enum ShardSocketOperationKind {
    Socket(Shard),
    Restore(Shard),
    Unsocket,
}
