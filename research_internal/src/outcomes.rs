use bevy::prelude::*;

use almanach::prelude::Almanach;
use logging::prelude::{Log, Tag};
use research::{
    model::{OutcomeDisplay, OutcomeSatisfied},
    outcomes::{FulfillOutcome, GrantShardBlueprint},
};
use shards::prelude::{ShardBlueprintAcquired, ShardBlueprints};

/// Derives the display projection and wires the fulfillment observer for any spawned outcome,
/// whether freshly instantiated or loaded from a save.
pub(crate) fn on_grant_shard_blueprint_add_init_outcome(
    trigger: On<Add, GrantShardBlueprint>,
    mut commands: Commands,
    almanach: Res<Almanach>,
    blueprints: Res<ShardBlueprints>,
    grants: Query<&GrantShardBlueprint>,
) {
    let entity = trigger.entity;
    let Ok(grant) = grants.get(entity) else { return };
    let info = almanach.get_shard_info(grant.0);
    commands.entity(entity)
        .insert(OutcomeDisplay {
            icon: info.icon.clone(),
            title: info.name.clone(),
        })
        .observe(on_fulfill_unlock_shard_blueprint);
    // This kind's satisfaction signal: the granted shard blueprint is already owned.
    if blueprints.is_unlocked(grant.0) {
        commands.entity(entity).insert(OutcomeSatisfied);
    }
}

/// This kind's reaction to its own possession event: mark outcomes whose shard was just acquired
/// as satisfied. (A future `ShardBlueprintRevoked` reaction would remove `OutcomeSatisfied`; the
/// generic aggregation would then un-obsolete any never-completed research.)
pub(crate) fn on_shard_blueprint_acquired_mark_outcomes_satisfied(
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

fn on_fulfill_unlock_shard_blueprint(
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
