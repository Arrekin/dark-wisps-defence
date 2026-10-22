//! # TimeBound expiry
//!
//! Despawns entities when their [`TimeBound`] expires.
//!
//! A min-heap orders expiries by end time to avoid scanning every entity each frame.
//! Inserting a `TimeBound` queues its expiry. Replacing or removing it leaves the old entry
//! in the queue. When an entry is due, it despawns the entity only if the current `TimeBound`
//! still has the queued end time.

use std::cmp::{Ordering, Reverse};
use std::collections::binary_heap::{BinaryHeap, PeekMut};

use bevy::prelude::*;

use session::{GameClock, TimeBound};
use states::prelude::{GameState, MapLoadingStage};

pub(crate) struct TimeBoundPlugin;
impl Plugin for TimeBoundPlugin {
    fn build(&self, app: &mut App) {
        app
            .init_resource::<TimeBoundExpiryQueue>()
            .add_systems(OnEnter(MapLoadingStage::Init), |mut commands: Commands| { commands.insert_resource(TimeBoundExpiryQueue::default()); })
            .add_systems(PostUpdate, expire_time_bounds.run_if(in_state(GameState::Running)))
            .add_observer(on_insert_time_bound_enqueue_expiry);
    }
}

fn on_insert_time_bound_enqueue_expiry(
    trigger: On<Insert<TimeBound>>,
    mut queue: ResMut<TimeBoundExpiryQueue>,
    time_bounds: Query<&TimeBound>,
) {
    let entity = trigger.entity;
    let Ok(time_bound) = time_bounds.get(entity) else { return; };
    queue.heap.push(Reverse(ExpiryEntry { ends_at: time_bound.ends_at, entity }));
}

fn expire_time_bounds(
    mut commands: Commands,
    mut queue: ResMut<TimeBoundExpiryQueue>,
    clock: Res<GameClock>,
    time_bounds: Query<&TimeBound>,
) {
    while let Some(next) = queue.heap.peek_mut() {
        if next.0.ends_at > clock.now() { break; }
        let Reverse(entry) = PeekMut::pop(next);
        let Ok(time_bound) = time_bounds.get(entry.entity) else { continue; };
        // Ignore an expiry superseded by a new end time.
        if time_bound.ends_at != entry.ends_at { continue; }
        commands.entity(entry.entity).try_despawn();
    }
}

/// Pending expiries ordered by end time, earliest first.
#[derive(Resource, Default)]
struct TimeBoundExpiryQueue {
    heap: BinaryHeap<Reverse<ExpiryEntry>>,
}

#[derive(PartialEq)]
struct ExpiryEntry {
    ends_at: f64,
    entity: Entity,
}
impl Eq for ExpiryEntry {}
impl PartialOrd for ExpiryEntry {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for ExpiryEntry {
    fn cmp(&self, other: &Self) -> Ordering {
        self.ends_at
            .total_cmp(&other.ends_at)
            .then(self.entity.to_bits().cmp(&other.entity.to_bits()))
    }
}
