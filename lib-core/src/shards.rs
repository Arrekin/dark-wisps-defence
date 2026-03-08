//! # Shard System
//!
//! Shards are player-insertable items that modify tower stats and behaviors.
//! Each tower type defines its own reaction to shard insertion via a per-entity observer
//! registered in its builder.
//!
//! ## Data Model
//! - `ShardSlots` component tracks which shards are socketed.
//! - `ShardApplyEvent` is triggered by `ShardSlots::insert` and handled per-tower.
use crate::lib_prelude::*;
use strum::Display;

pub mod shards_prelude {
    pub use super::{ShardApplyEvent, ShardEffect, ShardSlots, ShardType};
}

pub struct ShardsPlugin;
impl Plugin for ShardsPlugin {
    fn build(&self, _app: &mut App) {
    }
}

////////////////////
////  SHARD TYPE  //
////////////////////

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Display)]
pub enum ShardType {
    Range,
}

////////////////////
////  SHARD SLOTS //
////////////////////

/// Tracks which shards are currently socketed in a tower.
///
/// `insert` is the only public insertion API — it atomically updates slot state
/// and triggers `ShardApplyEvent` on the tower, ensuring the two operations are never
/// separated.
#[derive(Component)]
pub struct ShardSlots {
    capacity: usize,
    shards: Vec<ShardType>,
}
impl ShardSlots {
    pub fn new(capacity: usize) -> Self {
        Self { capacity, shards: Vec::new() }
    }

    /// Sockets a shard and triggers `ShardApplyEvent` on `tower_entity`.
    ///
    /// Caller must verify `!is_full()` before calling. Panics if the tower is at capacity.
    pub fn insert(
        &mut self,
        shard_type: ShardType,
        tower_entity: Entity,
        commands: &mut Commands,
    ) {
        assert!(!self.is_full(), "ShardSlots::insert called on full slots");
        self.shards.push(shard_type);
        commands.trigger(ShardApplyEvent { tower_entity, shard_type });
    }

    pub fn capacity(&self) -> usize {
        self.capacity
    }

    pub fn len(&self) -> usize {
        self.shards.len()
    }

    pub fn is_full(&self) -> bool {
        self.shards.len() >= self.capacity
    }

    pub fn iter(&self) -> impl Iterator<Item = ShardType> + '_ {
        self.shards.iter().copied()
    }
}

////////////////////////
////  SHARD EFFECT    //
////////////////////////

/// Marker on effect entities spawned by shard application.
///
/// Distinguishes shard effects from baseline effects, aura effects, debuffs, etc.
/// Used for removal queries, save/load filtering, and UI.
#[derive(Component)]
pub struct ShardEffect;
impl ShardEffect {
    /// Returns a bundle for a stat-only shard effect. Convenience for the common case
    /// where a shard simply contributes modifier values with no behavioral markers.
    pub fn from_modifiers(tower_entity: Entity, contributions: HashMap<ModifierType, f32>) -> impl Bundle {
        (EffectTarget(tower_entity), ModifierContributions(contributions), ShardEffect)
    }
}

///////////////////////
////  SHARD EVENTS   //
///////////////////////

/// Triggered by `ShardSlots::insert` when a shard is socketed.
///
/// Each tower type registers a per-entity observer for this event in its builder's `on_add`.
/// The observer is responsible for spawning the appropriate effect entity.
#[derive(EntityEvent, Clone, Copy)]
pub struct ShardApplyEvent {
    #[event_target]
    pub tower_entity: Entity,
    pub shard_type: ShardType,
}

