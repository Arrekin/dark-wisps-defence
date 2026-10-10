//! # Shard Orders
//!
//! Each order produces one shard. Root orders belong to the global queue or a Forge queue.
//! Ingredient orders supply the shards required by a parent order and can have their own
//! ingredients.
//!
//! A Forge starts an order once its ingredients are fulfilled and it can pay the pickup cost.
//! Starting consumes the ingredients. A fulfilled ingredient order holds its shard in a socket;
//! a fulfilled root order delivers to its destination socket, or to `Stock` if it has none.
//!
//! State is derived from components:
//! - Fulfilled: the order's socket holds its shard.
//! - In progress: the order has [`ForgedBy`].
//! - Waiting: neither of the above.

use std::time::Duration;

use bevy::prelude::*;

use game_core::prelude::{MapBound, Shard};

/// The shard this order produces. Marks the entity as an order.
#[derive(Component, Clone, Copy, Debug)]
#[component(immutable)]
#[require(MapBound)]
pub struct ShardOrder(pub Shard);

// ============================================================================
// QUEUES
// ============================================================================

/// Links a root order to its queue holder.
#[derive(Component)]
#[relationship(relationship_target = ForgingQueue)]
pub struct InForgingQueue(pub Entity);
impl InForgingQueue {
    /// Returns the Forge-queue color if `forge` holds this queue, otherwise the global-queue color.
    pub fn source_color(&self, forge: Entity) -> Color {
        if self.0 == forge { FORGE_QUEUE_COLOR } else { GLOBAL_QUEUE_COLOR }
    }
}

/// Root orders in placement order, including those in progress.
/// Absent when empty. Despawning the queue holder despawns its orders.
#[derive(Component)]
#[relationship_target(relationship = InForgingQueue, linked_spawn)]
pub struct ForgingQueue(Vec<Entity>);

/// Marks the singleton entity that holds the global queue.
#[derive(Component, Default)]
#[require(MapBound)]
pub struct GlobalForgingQueue;

// Job border colors identify the root order's queue.
pub const GLOBAL_QUEUE_COLOR: Color = Color::srgb(0.8, 0.6, 0.1); // amber
pub const FORGE_QUEUE_COLOR: Color = Color::srgb(0.2, 0.7, 0.8); // teal

/// Allows a Forge to take global orders when it cannot start any in its Forge queue.
/// Enabled by default.
#[derive(Component)]
pub struct WorksGlobalQueue(pub bool);
impl Default for WorksGlobalQueue {
    fn default() -> Self {
        Self(true)
    }
}
impl WorksGlobalQueue {
    pub fn toggle(&mut self) {
        self.0 = !self.0;
    }

    pub fn label(&self) -> &'static str {
        if self.0 { "Accept global orders: On" } else { "Accept global orders: Off" }
    }
}

// ============================================================================
// INGREDIENTS
// ============================================================================

/// Links an ingredient order to the parent order it supplies.
#[derive(Component)]
#[relationship(relationship_target = Ingredients)]
pub struct IngredientOf(pub Entity);

/// Ingredient orders, one per required shard. Despawning the parent order despawns them.
#[derive(Component)]
#[relationship_target(relationship = IngredientOf, linked_spawn)]
pub struct Ingredients(Vec<Entity>);

// ============================================================================
// FORGE LINK
// ============================================================================

/// Links an order to the Forge processing it. Removing the link resets its progress.
#[derive(Component)]
#[relationship(relationship_target = ForgeCurrentOrder)]
pub struct ForgedBy(pub Entity);

/// The order a Forge is processing. Absent when the Forge is idle.
#[derive(Component)]
#[relationship_target(relationship = ForgedBy)]
pub struct ForgeCurrentOrder(Entity);
impl ForgeCurrentOrder {
    pub fn order(&self) -> Entity {
        self.0
    }
}

/// Progress timer for an order being forged.
#[derive(Component)]
pub struct ForgingProgress {
    timer: Timer,
}
impl ForgingProgress {
    pub fn new(duration: Duration) -> Self {
        Self { timer: Timer::new(duration, TimerMode::Once) }
    }

    /// Restores saved time remaining, clamped to the current recipe duration.
    pub fn resumed(duration: Duration, remaining_secs: f32) -> Self {
        let mut timer = Timer::new(duration, TimerMode::Once);
        let elapsed = (duration.as_secs_f32() - remaining_secs).clamp(0.0, duration.as_secs_f32());
        timer.set_elapsed(Duration::from_secs_f32(elapsed));
        Self { timer }
    }

    /// Advances the timer; returns `true` on the tick it finishes.
    pub fn advance(&mut self, delta: Duration) -> bool {
        self.timer.tick(delta).just_finished()
    }

    /// From 0.0 (started) to 1.0 (finished).
    pub fn fraction(&self) -> f32 {
        self.timer.fraction()
    }

    pub fn remaining_secs(&self) -> f32 {
        self.timer.remaining_secs()
    }
}

// ============================================================================
// DESTINATION
// ============================================================================

/// Links a destination socket to the root order it awaits. Saved as part of the socket.
/// Removing or replacing the link leaves the order without a destination.
#[derive(Component)]
#[relationship(relationship_target = ShardOrderDestination)]
pub struct AwaitsShardOrder(pub Entity);

/// A root order's destination socket. Without one, the order delivers to `Stock`.
#[derive(Component)]
#[relationship_target(relationship = AwaitsShardOrder)]
pub struct ShardOrderDestination(Entity);
impl ShardOrderDestination {
    pub fn socket(&self) -> Entity {
        self.0
    }
}

// ============================================================================
// BUILDER
// ============================================================================

/// Attaches an order to a queue or to a parent order as an ingredient.
#[derive(Clone, Copy)]
pub enum OrderLink {
    Queue(Entity),
    IngredientOf(Entity),
}

/// Creates an order. Ingredient gathering starts after map loading finishes.
#[derive(Component)]
pub struct BuilderShardOrder {
    pub shard: Shard,
    pub link: OrderLink,
    /// Saved Forge and remaining seconds. `None` starts without an active job.
    pub in_progress: Option<(Entity, f32)>,
}
impl BuilderShardOrder {
    pub fn new(shard: Shard, link: OrderLink) -> Self {
        Self { shard, link, in_progress: None }
    }

    pub fn with_in_progress(mut self, in_progress: impl Into<Option<(Entity, f32)>>) -> Self {
        self.in_progress = in_progress.into();
        self
    }
}

// ============================================================================
// REQUESTS
// ============================================================================

/// Cancels an order and its ingredients according to `mode`. Held shards return to `Stock`.
#[derive(EntityEvent, Clone, Copy)]
pub struct ShardOrderCancelRequest {
    #[event_target]
    pub order: Entity,
    pub mode: ShardOrderCancelMode,
}

/// How cancellation handles jobs already in progress.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ShardOrderCancelMode {
    /// Discards active jobs without refunding their consumed resources.
    Hard,
    /// Keeps active jobs as root orders in the same queue.
    Soft,
}

/// Selects a Forge queue in the forging panel, or the global queue for `None`.
#[derive(Event, Clone, Copy, Debug)]
pub struct ForgingPanelSelectRequest {
    pub forge: Option<Entity>,
}
