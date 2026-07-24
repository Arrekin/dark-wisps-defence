use bevy::prelude::*;

use crate::components::MapBound;
use crate::prelude::FromEntity;

// ============================================================================
// Moments
//
// A moment is a scenario-relevant point in time, represented as an entity.
// The owning domain fires `MomentHappened` on the moment entity when the fact
// it records occurs. The generic propagator catches it, walks `MomentWatchers`,
// and fires `MomentHappened` on each watcher. What the watcher does in response
// is the watcher's domain business — the moment system stops at delivering
// `MomentHappened`.
//
// Moment kinds are distributed across domains: each domain defines its own
// marker type + `MomentKind` impl, spawns moment children on its entities, and
// self-fires by registering a domain-event listener on the parent at spawn time.
// ============================================================================

/// Marker component on moment entities. Carries `fired_count` — universal
/// firing state, persisted across saves.
#[derive(Component, Default)]
#[require(MapBound)]
pub struct Moment {
    pub fired_count: u32,
}

impl Moment {
    pub fn fire(&mut self, entity_commands: &mut EntityCommands) {
        self.fired_count += 1;
        entity_commands.trigger(MomentHappened::from);
    }

    /// No-op if already fired. For one-shot moments that must not re-fire on
    /// reload.
    pub fn fire_if_not_yet_fired(&mut self, entity_commands: &mut EntityCommands) {
        if self.fired_count >= 1 { return; }
        self.fire(entity_commands);
    }
}

/// Source side (on the moment): "this moment belongs to parent P."
/// Linked despawn ON — moments die with their parent.
/// Standalone moments are self-parented (`MomentOf(self)`).
#[derive(Component)]
#[relationship(relationship_target = HasMoments, allow_self_referential)]
pub struct MomentOf(pub Entity);

/// Target side (on the parent): collection of moments owned by this entity.
/// `linked_spawn` causes despawn cascade — when the parent despawns, all its
/// moments despawn too.
#[derive(Component)]
#[relationship_target(relationship = MomentOf, linked_spawn)]
pub struct HasMoments(Vec<Entity>);

/// Source side (on the watcher): "this watcher is interested in moment M."
/// Linked despawn OFF — watching is a reference, not ownership. When the
/// moment despawns, the relationship is removed (firing `On<Remove>`), but
/// the watcher is NOT despawned (lost-watcher rule, handled by each domain).
#[derive(Component)]
#[relationship(relationship_target = MomentWatchers)]
pub struct MomentOfInterest(pub Entity);

/// Target side (on the moment): collection of watchers interested in this
/// moment. No `linked_spawn` — despawning the moment does NOT despawn
/// watchers. This is what the generic propagator walks.
#[derive(Component)]
#[relationship_target(relationship = MomentOfInterest)]
pub struct MomentWatchers(Vec<Entity>);

/// Fired on a moment entity when the fact it records has occurred. Caught by
/// the generic propagator (which forwards it to watchers) and by domain
/// reactors (which respond to it).
#[derive(EntityEvent, Clone, Copy, FromEntity)]
pub struct MomentHappened {
    #[event_target]
    pub entity: Entity,
}

/// Trait for moment kind markers. Each domain defines its own marker types
/// implementing this trait. Use `#[derive(MomentKind)]` — do not implement
/// manually. The derive infers `KIND` from the type name:
/// `MomentObjectiveSatisfied` → `"objective_satisfied"`.
///
/// ```
/// #[derive(Component, Default, MomentKind)]
/// pub struct MomentGameStart; // KIND = "game_start"
/// ```
pub trait MomentKind: Component + Default {
    /// Persistence key for this moment kind in the `moments` table. Inferred
    /// from the type name by the `MomentKind` derive macro.
    const KIND: &'static str;
}

// ============================================================================
// Self-firing helpers
//
// Moment children self-fire by registering a domain-event listener on their
// parent at spawn time. When the parent fires its domain event, the listener
// fires the moment. These helpers eliminate the boilerplate of writing the
// same closure + observer pair for every moment kind.
// ============================================================================

/// Returns a closure that fires the moment when event `E` hits the parent.
/// Self-despawns its own observer the next time the event fires after the
/// moment is gone.
pub fn fire_moment_on<E: EntityEvent>(
    moment_entity: Entity,
) -> impl Fn(On<E>, Commands, Query<&mut Moment>) {
    move |trigger: On<E>, mut commands: Commands, mut moments: Query<&mut Moment>| {
        let Ok(mut moment) = moments.get_mut(moment_entity) else {
            commands.entity(trigger.observer()).try_despawn();
            return;
        };
        moment.fire(&mut commands.entity(moment_entity));
    }
}

/// Observer for `On<Add, T>`: reads `MomentOf(parent)` from the moment child
/// and registers `fire_moment_on::<E>` on the parent.
/// Register directly: `app.add_observer(moment_attach_self_trigger_to_parent::<T, E>)`.
pub fn moment_attach_self_trigger_to_parent<T: Component, E: EntityEvent>(
    trigger: On<Add, T>,
    mut commands: Commands,
    moments: Query<&MomentOf>,
) {
    let moment_entity = trigger.entity;
    let Ok(parent_rel) = moments.get(moment_entity) else { return };
    let parent = parent_rel.0;
    commands.entity(parent).observe(fire_moment_on::<E>(moment_entity));
}
