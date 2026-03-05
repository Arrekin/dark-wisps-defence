//! # Effects System
//!
//! Three-layer architecture: effect instance entities → ModifierBank → derived components.
//!
//! Effect instances carry `EffectTarget` (who they modify) and `ModifierContributions`
//! (what they contribute).

use std::cmp::Reverse;
use std::collections::BinaryHeap;

use crate::game_clock::GameClock;

use crate::lib_prelude::*;

pub mod effects_prelude {
    pub use super::{
        BaselineEffect, EffectInstances, EffectSource, EffectSourceOf, EffectTarget,
        ExpiresAt, ModifierContributions,
    };
}

pub struct EffectsPlugin;
impl Plugin for EffectsPlugin {
    fn build(&self, app: &mut App) {
        app
            .init_resource::<EffectsExpiryQueue>()
            .add_systems(PostUpdate,
                EffectsExpiryQueue::process.run_if(in_state(GameState::Running)),
            )
            .add_observer(ExpiresAt::on_insert)
            ;
    }
}

////////////////////////////
// EFFECT TARGET RELATION //
////////////////////////////

/// Relationship from an effect instance entity to the entity whose stats it modifies.
#[derive(Component)]
#[relationship(relationship_target = EffectInstances)]
pub struct EffectTarget(pub Entity);

/// Inverse of `EffectTarget`.
///
/// `linked_spawn` ensures that when this entity is despawned, all effect instances
/// targeting it are cascade-despawned.
#[derive(Component, Default)]
#[relationship_target(relationship = EffectTarget, linked_spawn)]
pub struct EffectInstances(Vec<Entity>);

////////////////////////////
// EFFECT SOURCE RELATION //
////////////////////////////

/// Relationship from an effect instance entity to the entity that spawned it.
///
/// `linked_spawn` on `EffectSourceOf` ensures despawning the source cascades to all
/// effect instances it owns.
#[derive(Component)]
#[relationship(relationship_target = EffectSourceOf)]
pub struct EffectSource(pub Entity);

/// Inverse of `EffectSource`.
#[derive(Component, Default)]
#[relationship_target(relationship = EffectSource, linked_spawn)]
pub struct EffectSourceOf(Vec<Entity>);

/////////////////////
// MODIFIER CONTRIBUTIONS //
/////////////////////

/// The set of stat contributions an effect instance provides to its target.
#[derive(Component)]
pub struct ModifierContributions(pub HashMap<ModifierType, f32>);

/// Marker on the permanent baseline effect instance spawned at entity creation.
///
/// Baseline effects are never saved; they are always reconstructed when the entity spawns.
#[derive(Component)]
pub struct BaselineEffect;

/////////////////
// EXPIRY      //
/////////////////

/// Absolute game-time at which the effect instance is despawned.
#[derive(Component, Clone, Copy, PartialEq)]
pub struct ExpiresAt(pub f64);
impl ExpiresAt {
    fn on_insert(
        trigger: On<Insert, ExpiresAt>,
        mut queue: ResMut<EffectsExpiryQueue>,
        expires: Query<&ExpiresAt>,
    ) {
        let entity = trigger.entity;
        let Ok(expires_at) = expires.get(entity) else { return; };
        queue.push(*expires_at, entity);
    }
}

//////////////////
// EXPIRY QUEUE //
//////////////////

#[derive(PartialEq)]
struct EffectExpiryEntry {
    expires_at: ExpiresAt,
    entity: Entity,
}
impl Eq for EffectExpiryEntry {}
impl PartialOrd for EffectExpiryEntry {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for EffectExpiryEntry {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.expires_at.0
            .total_cmp(&other.expires_at.0)
            .then(self.entity.to_bits().cmp(&other.entity.to_bits()))
    }
}

/// Global min-heap of pending effect expirations, ordered by absolute game time.
///
/// Tombstone pattern: entries removed early (via entity despawn) are ignored when popped.
#[derive(Resource, Default)]
struct EffectsExpiryQueue {
    heap: BinaryHeap<Reverse<EffectExpiryEntry>>,
}
impl EffectsExpiryQueue {
    fn push(&mut self, expires_at: ExpiresAt, entity: Entity) {
        self.heap.push(Reverse(EffectExpiryEntry { expires_at, entity }));
    }

    fn process(
        mut commands: Commands,
        clock: Res<GameClock>,
        mut queue: ResMut<EffectsExpiryQueue>,
        entities: Query<Entity>,
    ) {
        while let Some(Reverse(entry)) = queue.heap.peek() {
            if entry.expires_at.0 > clock.elapsed {
                break;
            }
            let entry = queue.heap.pop().unwrap().0;
            if entities.contains(entry.entity) {
                commands.entity(entry.entity).despawn();
            }
        }
    }
}
