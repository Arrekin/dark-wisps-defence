pub(crate) mod visual;
pub(crate) mod brittle;
pub(crate) mod slow;

use std::cmp::Reverse;
use std::collections::BinaryHeap;

use bevy::prelude::*;

use alteration::effects::ExpiresAt;
use session::GameClock;
use states::GameState;

pub struct EffectsPlugin;
impl Plugin for EffectsPlugin {
    fn build(&self, app: &mut App) {
        app
            .init_resource::<EffectsExpiryQueue>()
            .add_systems(PostUpdate,
                EffectsExpiryQueue::process.run_if(in_state(GameState::Running)),
            )
            .add_observer(enqueue_effect_expiry_on_insert)
            .add_plugins((
                visual::EffectVisualsPlugin,
                brittle::BrittleEffectPlugin,
                slow::SlowEffectPlugin,
            ));
    }
}

fn enqueue_effect_expiry_on_insert(
    trigger: On<Insert, ExpiresAt>,
    mut queue: ResMut<EffectsExpiryQueue>,
    expires: Query<&ExpiresAt>,
) {
    let entity = trigger.entity;
    let Ok(expires_at) = expires.get(entity) else { return; };
    queue.push(*expires_at, entity);
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
        mut queue: ResMut<EffectsExpiryQueue>,
        clock: Res<GameClock>,
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
