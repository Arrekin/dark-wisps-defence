use bevy::{
    platform::collections::HashMap,
    prelude::*,
};
use strum::EnumCount;

use alteration::modifiers::prelude::ModifierType;
use game_core::prelude::{ContentId, Shard, ShardTier, ShardType};

/// Socket definition: accepted shard and modifier contributions per tier.
///
/// Each socket owns its copy, saved with it, so Almanach changes do not affect existing sockets.
/// Create or redefine sockets via `ShardSocketUpsert`, change contents via `ShardSocketOperation`.
#[derive(Component, Clone)]
#[component(immutable)]
pub struct ShardSocket {
    pub shard_type: ShardType,
    /// `None` accepts any tier.
    pub shard_tier: Option<ShardTier>,
    /// Player-facing, e.g. "Attack speed".
    pub description: String,
    /// Indexed by tier, T1 first. Applied to the holder.
    pub contributions: [HashMap<ModifierType, f32>; ShardTier::COUNT],
}
impl ShardSocket {
    /// Accepts any tier of `shard_type`, contributing `per_tier[tier]` to a single modifier.
    pub fn new(shard_type: ShardType, description: impl Into<String>, modifier: ModifierType, per_tier: [f32; ShardTier::COUNT]) -> Self {
        Self {
            shard_type,
            shard_tier: None,
            description: description.into(),
            contributions: per_tier.map(|value| HashMap::from([(modifier, value)])),
        }
    }

    /// Accepts any tier of `shard_type`, contributes nothing.
    pub fn without_contributions(shard_type: ShardType, description: impl Into<String>) -> Self {
        Self { shard_type, shard_tier: None, description: description.into(), contributions: Default::default() }
    }

    pub fn accepts(&self, shard: Shard) -> bool {
        shard.shard_type == self.shard_type && self.shard_tier.is_none_or(|tier| tier == shard.tier)
    }

    pub fn contributions_for(&self, tier: ShardTier) -> &HashMap<ModifierType, f32> {
        &self.contributions[tier as usize]
    }
}

/// Socket → holder. Despawning the holder despawns its sockets.
#[derive(Component)]
#[relationship(relationship_target = ShardSockets)]
pub struct ShardSocketOf(pub Entity);

/// Holder's sockets.
#[derive(Component)]
#[relationship_target(relationship = ShardSocketOf, linked_spawn)]
pub struct ShardSockets(Vec<Entity>);

/// A socket removed from its holder: accepts no shards, contributes nothing, hidden in the UI.
#[derive(Component, Default)]
pub struct RemovedSocket;

/// Shard currently in the socket. Inserting it spawns the tier's `ShardEffect` on the holder, with
/// the socket as `EffectSource`; replacing or removing it despawns that effect and hands the shard
/// over to `Stock`, also when the socket despawns with its holder.
#[derive(Component, Clone, Copy, Debug)]
#[component(immutable)]
pub struct SocketedShard(pub Shard);

/// Creates the holder's socket with `content_id`, or redefines the existing one. The only way
/// sockets are created. The socket's current shard returns to `Stock`, then `shard` is placed in
/// it; a `shard` the socket does not accept goes to `Stock`.
#[derive(EntityEvent, Clone)]
pub struct ShardSocketUpsert {
    #[event_target]
    pub holder: Entity,
    pub content_id: ContentId,
    pub definition: ShardSocket,
    pub removed: bool,
    pub shard: Option<Shard>,
}
impl ShardSocketUpsert {
    /// An empty, not removed socket.
    pub fn new(holder: Entity, content_id: ContentId, definition: ShardSocket) -> Self {
        Self { holder, content_id, definition, removed: false, shard: None }
    }

    pub fn with_removed(mut self, removed: bool) -> Self {
        self.removed = removed;
        self
    }

    pub fn with_shard(mut self, shard: impl Into<Option<Shard>>) -> Self {
        self.shard = shard.into();
        self
    }
}

/// Player-side socket change, moving shards between `Stock` and the socket:
/// - `socket`: takes the shard from `Stock`. Ignored if not accepted, removed or not in `Stock`.
/// - `unsocket`: empties the socket.
///
/// A replaced or unsocketed shard returns to `Stock`.
#[derive(EntityEvent, Clone, Copy)]
pub struct ShardSocketOperation {
    #[event_target]
    pub socket: Entity,
    pub kind: ShardSocketOperationKind,
}
impl ShardSocketOperation {
    pub fn socket(socket: Entity, shard: Shard) -> Self {
        Self { socket, kind: ShardSocketOperationKind::Socket(shard) }
    }

    pub fn unsocket(socket: Entity) -> Self {
        Self { socket, kind: ShardSocketOperationKind::Unsocket }
    }
}

#[derive(Clone, Copy, Debug)]
pub enum ShardSocketOperationKind {
    Socket(Shard),
    Unsocket,
}
