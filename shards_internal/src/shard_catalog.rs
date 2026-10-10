//! # Shard Catalog
//!
//! Registers every shard type and tier in the Almanach, with display metadata in `ResourceInfo`
//! and a forging recipe in `ShardInfo`. Recipe costs and durations are provisional balance values.

use std::time::Duration;

use bevy::prelude::*;
use strum::IntoEnumIterator;

use almanach::{ResourceInfo, ShardInfo, ShardRecipe, prelude::AlmanachAppExt};
use game_core::prelude::{Shard, ShardTier, ShardType};
use resources::prelude::{ResourceAmount, ResourceType};

// Recipe tuning
const LOWER_TIER_SHARDS_PER_RECIPE: i32 = 3;

pub(crate) struct ShardCatalogPlugin;
impl Plugin for ShardCatalogPlugin {
    fn build(&self, app: &mut App) {
        let families = [
            (ShardType::Strength, "Strength", "Peace was never an option.", "ui/shards/shard_strength.png"),
            (ShardType::Speed, "Speed", "Go fast. Go faster.", "ui/shards/shard_speed.png"),
            (ShardType::Reach, "Reach", "Distance is just a concept. Ignore it.", "ui/shards/shard_reach.png"),
            (ShardType::Fire, "Fire", "Burn it all down.", "ui/shards/shard_fire.png"),
            (ShardType::Water, "Water", "Flow like water.", "ui/shards/shard_water.png"),
            (ShardType::Light, "Light", "Illuminate the darkness.", "ui/shards/shard_light.png"),
            (ShardType::Electric, "Electric", "Shock and awe.", "ui/shards/shard_electric.png"),
        ];
        for (shard_type, name, description, icon_path) in families {
            let icon: Handle<Image> = app.world().resource::<AssetServer>().load(icon_path);
            for tier in ShardTier::iter() {
                let (dark_ore, duration_secs) = match tier {
                    ShardTier::T1 => (100, 8),
                    ShardTier::T2 => (200, 12),
                    ShardTier::T3 => (400, 16),
                };
                let pickup_cost = vec![ResourceAmount::new(ResourceType::DarkOre, dark_ore)];
                let reserved_cost = tier.below()
                    .map(|below| ResourceAmount::new(Shard::new(shard_type, below), LOWER_TIER_SHARDS_PER_RECIPE))
                    .into_iter()
                    .collect();
                app.register_shard(
                    Shard::new(shard_type, tier),
                    ResourceInfo {
                        name: format!("{name} {tier}"),
                        description: description.to_string(),
                        icon: icon.clone(),
                    },
                    ShardInfo {
                        recipe: Some(ShardRecipe { pickup_cost, reserved_cost, duration: Duration::from_secs(duration_secs) }),
                    },
                );
            }
        }
    }
}
