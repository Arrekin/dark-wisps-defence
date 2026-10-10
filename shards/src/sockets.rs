use bevy::{
    platform::collections::HashMap,
    prelude::*,
};
use strum::{AsRefStr, EnumCount, EnumString};

use alteration::modifiers::prelude::ModifierType;
use game_core::prelude::{ContentId, Shard, ShardTier, ShardType};

/// Accepted shard type, optional tier restriction, and modifier contributions per tier.
///
/// Saved with the socket, so Almanach changes do not alter existing sockets.
/// Use `ShardSocketUpsert` to create or redefine a socket and `ShardSocketOperation` to change its contents.
#[derive(Component, Clone)]
#[component(immutable)]
#[require(ShardReleaseDestination, ShardSocketState)]
pub struct ShardSocket {
    pub shard_type: ShardType,
    /// `None` accepts any tier.
    pub shard_tier: Option<ShardTier>,
    /// Player-facing label, e.g. "Attack speed".
    pub description: String,
    /// Modifiers applied to the holder, indexed by tier starting at T1.
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

    /// Accepts any tier of `shard_type` with no modifier contributions.
    pub fn without_contributions(shard_type: ShardType, description: impl Into<String>) -> Self {
        Self { shard_type, shard_tier: None, description: description.into(), contributions: Default::default() }
    }

    /// Accepts only `shard` with no modifier contributions.
    pub fn exactly(shard: Shard, description: impl Into<String>) -> Self {
        Self { shard_type: shard.shard_type, shard_tier: Some(shard.tier), description: description.into(), contributions: Default::default() }
    }

    pub fn accepts(&self, shard: Shard) -> bool {
        shard.shard_type == self.shard_type && self.shard_tier.is_none_or(|tier| tier == shard.tier)
    }

    pub fn contributions_for(&self, tier: ShardTier) -> &HashMap<ModifierType, f32> {
        &self.contributions[tier as usize]
    }
}

/// Links a socket to its holder. Despawning the holder despawns its sockets.
#[derive(Component)]
#[relationship(relationship_target = ShardSockets)]
pub struct ShardSocketOf(pub Entity);

#[derive(Component)]
#[relationship_target(relationship = ShardSocketOf, linked_spawn)]
pub struct ShardSockets(Vec<Entity>);

/// Where a socket sends its shard when the shard is removed, replaced, or despawned with the socket.
/// Also used as the destination of `ShardSocketOperation::Socket`.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ShardReleaseDestination {
    #[default]
    Stock,
    Socket(Entity),
    /// Consumes the shard without returning it anywhere.
    Void,
}
impl From<Entity> for ShardReleaseDestination {
    fn from(socket: Entity) -> Self {
        Self::Socket(socket)
    }
}

/// Controls a socket's contents and effects:
/// - `Active`: holds a shard and applies its modifiers to the holder.
/// - `Disabled`: holds a shard without applying modifiers.
/// - `Removed`: rejects shards and is hidden in the UI; entering this state empties the socket.
///
/// Inserting a state also replaces the query marker with the corresponding `*Socket` component.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq, AsRefStr, EnumString)]
#[component(immutable)]
pub enum ShardSocketState {
    #[default]
    Active,
    Disabled,
    Removed,
}

/// Marks a socket in `ShardSocketState::Active`.
#[derive(Component)]
pub struct ActiveSocket;

/// Marks a socket in `ShardSocketState::Disabled`.
#[derive(Component)]
pub struct DisabledSocket;

/// Marks a socket in `ShardSocketState::Removed`.
#[derive(Component)]
pub struct RemovedSocket;

/// The shard held by a socket. Inserting it reapplies the socket's state.
/// Removing or replacing it releases the shard to `ShardReleaseDestination` and clears its effect.
/// Despawning the holder also releases the shard and removes the effect.
#[derive(Component, Clone, Copy, Debug)]
#[component(immutable)]
pub struct SocketedShard(pub Shard);

/// Creates or redefines the holder's socket identified by `content_id`.
/// Releases the existing shard and detaches the awaited order before inserting the new contents.
/// Applies the definition and state last; `Removed` clears the new contents too.
#[derive(EntityEvent, Clone)]
pub struct ShardSocketUpsert {
    #[event_target]
    pub holder: Entity,
    pub content_id: ContentId,
    pub definition: ShardSocket,
    pub state: ShardSocketState,
    /// A shard already removed from its source. The caller must ensure the definition accepts it.
    pub shard: Option<Shard>,
    /// The root order whose shard the socket awaits.
    pub awaited_order: Option<Entity>,
}
impl ShardSocketUpsert {
    /// Creates a request for an empty, active socket.
    pub fn new(holder: Entity, content_id: ContentId, definition: ShardSocket) -> Self {
        Self { holder, content_id, definition, state: ShardSocketState::Active, shard: None, awaited_order: None }
    }

    pub fn with_state(mut self, state: ShardSocketState) -> Self {
        self.state = state;
        self
    }

    pub fn with_shard(mut self, shard: impl Into<Option<Shard>>) -> Self {
        self.shard = shard.into();
        self
    }

    pub fn with_awaited_order(mut self, order: impl Into<Option<Entity>>) -> Self {
        self.awaited_order = order.into();
        self
    }
}

/// Changes socket contents, keeping a held shard and an awaited order mutually exclusive.
/// A global event because destinations include `Stock`, which is not an entity.
#[derive(Event, Clone, Copy, Debug)]
pub enum ShardSocketOperation {
    /// Transfers a shard already removed from its source. Missing, removed, or incompatible sockets
    /// return it to `Stock`. An accepting socket releases its previous shard and detaches its awaited
    /// order, then stores the new shard.
    Socket { shard: Shard, destination: ShardReleaseDestination },
    /// Releases the held shard to `ShardReleaseDestination` and detaches the awaited order.
    Unsocket { socket: Entity },
    /// Empties the socket, then links it to the order. Ignored if the socket is removed.
    AwaitOrder { socket: Entity, order: Entity },
}
impl ShardSocketOperation {
    pub fn socket(destination: impl Into<ShardReleaseDestination>, shard: Shard) -> Self {
        Self::Socket { shard, destination: destination.into() }
    }

    pub fn unsocket(socket: Entity) -> Self {
        Self::Unsocket { socket }
    }

    pub fn await_order(socket: Entity, order: Entity) -> Self {
        Self::AwaitOrder { socket, order }
    }
}
