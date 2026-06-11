//! Research outcomes: heterogeneous typed child entities that each own their own fulfillment and
//! their own persistence lane.
//!
//! On completion the research triggers [`FulfillOutcome`] on each outcome child; each outcome type's
//! per-entity observer reacts in its own lane. The research never matches on outcome type.
//! Fulfillment is edge-triggered — observers act only in response to the event, never on spawn — so
//! re-creating outcome entities on load does not re-grant anything.
//!
//! Each outcome kind:
//! - is a component with an `on_add` observer that derives its `OutcomeDisplay` and attaches its
//!   fulfillment observer (so fresh-spawned and loaded outcomes are wired identically),
//! - reacts to `FulfillOutcome` to perform its grant,
//! - owns its persistence (see `research_persistence`).

use crate::lib_prelude::*;

use crate::research::{OutcomeDisplay, OutcomeSatisfied};

/// Triggered on an outcome entity when its parent research completes.
#[derive(EntityEvent, Clone, Copy)]
pub struct FulfillOutcome {
    #[event_target]
    pub outcome: Entity,
}

/// Outcome leaf: grants a shard blueprint when its research completes.
#[derive(Component)]
pub struct GrantShardBlueprint(pub ShardType);
impl GrantShardBlueprint {
    /// Derives the display projection and wires the fulfillment observer for any spawned outcome,
    /// whether freshly instantiated or loaded from a save.
    pub fn on_add(
        trigger: On<Add, GrantShardBlueprint>,
        mut commands: Commands,
        grants: Query<&GrantShardBlueprint>,
        almanach: Res<Almanach>,
        blueprints: Res<ShardBlueprints>,
    ) {
        let entity = trigger.entity;
        let Ok(grant) = grants.get(entity) else { return };
        let info = almanach.get_shard_info(grant.0);
        commands.entity(entity)
            .insert(OutcomeDisplay {
                icon: info.icon.clone(),
                title: info.name.clone(),
            })
            .observe(Self::on_fulfill);
        // This kind's satisfaction signal: the granted shard blueprint is already owned.
        if blueprints.is_unlocked(grant.0) {
            commands.entity(entity).insert(OutcomeSatisfied);
        }
    }

    /// This kind's reaction to its own possession event: mark outcomes whose shard was just acquired
    /// as satisfied. (A future `ShardBlueprintRevoked` reaction would remove `OutcomeSatisfied`; the
    /// generic aggregation would then un-obsolete any never-completed research.)
    pub fn on_shard_blueprint_acquired(
        trigger: On<ShardBlueprintAcquired>,
        mut commands: Commands,
        outcomes: Query<(Entity, &GrantShardBlueprint), Without<OutcomeSatisfied>>,
    ) {
        let acquired = trigger.event().0;
        for (entity, grant) in outcomes.iter() {
            if grant.0 == acquired {
                commands.entity(entity).insert(OutcomeSatisfied);
            }
        }
    }

    fn on_fulfill(
        trigger: On<FulfillOutcome>,
        mut commands: Commands,
        mut blueprints: ResMut<ShardBlueprints>,
        grants: Query<&GrantShardBlueprint>,
    ) {
        let Ok(grant) = grants.get(trigger.outcome) else { return };
        if blueprints.unlock(grant.0) {
            commands.trigger(ShardBlueprintAcquired(grant.0));
            Log::info().player().tag(Tag::Research).message(format!("Research granted shard blueprint: {}", grant.0));
        }
    }
}
