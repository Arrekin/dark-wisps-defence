use std::time::Duration;

use bevy::prelude::*;

use game_core::prelude::*;
use outcomes::prelude::*;
use research::prelude::*;
use resources::prelude::*;
use shards::prelude::UnlockShardBlueprint;

pub fn spawn_fire_shard_recipe_research(commands: &mut Commands, id: &ContentId) {
    commands.spawn_scene(bsn! {
        Research {
            cost: {vec![Cost { resource_type: ResourceType::Essence(EssenceType::Fire), amount: 100 }]},
            duration: {Duration::from_secs(30)},
        }
        ContentId({id.0.clone()})
        DisplayName("Fire Shard Recipe")
        DisplayDescription("Unlocks the blueprint to forge Fire shards.")
        DisplayIconSwitcher("ui/shards/shard_fire.png")
        HasOutcomes [
            UnlockShardBlueprint({ShardType::Fire})
        ]
    });
}

pub fn spawn_water_shard_recipe_research(commands: &mut Commands, id: &ContentId) {
    commands.spawn_scene(bsn! {
        Research {
            cost: {vec![Cost { resource_type: ResourceType::Essence(EssenceType::Water), amount: 100 }]},
            duration: {Duration::from_secs(30)},
        }
        ContentId({id.0.clone()})
        DisplayName("Water Shard Recipe")
        DisplayDescription("Unlocks the blueprint to forge Water shards.")
        DisplayIconSwitcher("ui/shards/shard_water.png")
        HasOutcomes [
            UnlockShardBlueprint({ShardType::Water})
        ]
    });
}

pub fn spawn_light_shard_recipe_research(commands: &mut Commands, id: &ContentId) {
    commands.spawn_scene(bsn! {
        Research {
            cost: {vec![Cost { resource_type: ResourceType::Essence(EssenceType::Light), amount: 100 }]},
            duration: {Duration::from_secs(30)},
        }
        ContentId({id.0.clone()})
        DisplayName("Light Shard Recipe")
        DisplayDescription("Unlocks the blueprint to forge Light shards.")
        DisplayIconSwitcher("ui/shards/shard_light.png")
        HasOutcomes [
            UnlockShardBlueprint({ShardType::Light})
        ]
    });
}

pub fn spawn_electric_shard_recipe_research(commands: &mut Commands, id: &ContentId) {
    commands.spawn_scene(bsn! {
        Research {
            cost: {vec![Cost { resource_type: ResourceType::Essence(EssenceType::Electric), amount: 100 }]},
            duration: {Duration::from_secs(30)},
        }
        ContentId({id.0.clone()})
        DisplayName("Electric Shard Recipe")
        DisplayDescription("Unlocks the blueprint to forge Electric shards.")
        DisplayIconSwitcher("ui/shards/shard_electric.png")
        HasOutcomes [
            UnlockShardBlueprint({ShardType::Electric})
        ]
    });
}
