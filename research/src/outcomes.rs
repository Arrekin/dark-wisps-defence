use bevy::prelude::*;
use game_core::prelude::ShardType;

/// Triggered on an outcome entity when its parent research completes.
#[derive(EntityEvent, Clone, Copy)]
pub struct FulfillOutcome {
    #[event_target]
    pub outcome: Entity,
}

/// Outcome leaf: grants a shard blueprint when its research completes.
#[derive(Component)]
pub struct GrantShardBlueprint(pub ShardType);
