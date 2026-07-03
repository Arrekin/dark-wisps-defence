//! # Shard Catalog
//!
//! Registers per-shard metadata (name, description, icon, forge recipe) into the
//! [`Almanach`] at startup — the single source of truth that shard UI and crafting read
//! from. Lives alongside the other shard state (`shard_inventory`, `shard_blueprints`).
//!
//! Recipe costs and durations are placeholder tuning; adjust during a global balance pass.

use std::time::Duration;

use bevy::prelude::*;

use almanach::prelude::{AlmanachAppExt, ShardInfo, ShardRecipe};
use game_core::prelude::ShardType;
use resources::prelude::{Cost, ResourceType};

pub struct ShardCatalogPlugin;
impl Plugin for ShardCatalogPlugin {
    fn build(&self, app: &mut App) {
        let asset_server = app.world().resource::<AssetServer>();
        let range_shard_image: Handle<Image> = asset_server.load("ui/shards/shard_range.png");
        let damage_shard_image: Handle<Image> = asset_server.load("ui/shards/shard_damage.png");
        let speed_shard_image: Handle<Image> = asset_server.load("ui/shards/shard_speed.png");
        let fire_shard_image: Handle<Image> = asset_server.load("ui/shards/shard_fire.png");
        let water_shard_image: Handle<Image> = asset_server.load("ui/shards/shard_water.png");
        let light_shard_image: Handle<Image> = asset_server.load("ui/shards/shard_light.png");
        let electric_shard_image: Handle<Image> = asset_server.load("ui/shards/shard_electric.png");
        app
            .register_shard(ShardType::Range, ShardInfo {
                name: "Range".to_string(),
                description: "Distance is just a concept. Ignore it.".to_string(),
                icon: range_shard_image,
                recipe: Some(ShardRecipe {
                    cost: vec![Cost { resource_type: ResourceType::DarkOre, amount: 100 }],
                    duration: Duration::from_secs(8),
                }),
            })
            .register_shard(ShardType::Damage, ShardInfo {
                name: "Damage".to_string(),
                description: "Peace was never an option.".to_string(),
                icon: damage_shard_image,
                recipe: Some(ShardRecipe {
                    cost: vec![Cost { resource_type: ResourceType::DarkOre, amount: 100 }],
                    duration: Duration::from_secs(8),
                }),
            })
            .register_shard(ShardType::Speed, ShardInfo {
                name: "Speed".to_string(),
                description: "Go fast. Go faster.".to_string(),
                icon: speed_shard_image,
                recipe: Some(ShardRecipe {
                    cost: vec![Cost { resource_type: ResourceType::DarkOre, amount: 100 }],
                    duration: Duration::from_secs(8),
                }),
            })
            .register_shard(ShardType::Fire, ShardInfo {
                name: "Fire".to_string(),
                description: "Burn it all down.".to_string(),
                icon: fire_shard_image,
                recipe: Some(ShardRecipe {
                    cost: vec![Cost { resource_type: ResourceType::DarkOre, amount: 100 }],
                    duration: Duration::from_secs(8),
                }),
            })
            .register_shard(ShardType::Water, ShardInfo {
                name: "Water".to_string(),
                description: "Flow like water.".to_string(),
                icon: water_shard_image,
                recipe: Some(ShardRecipe {
                    cost: vec![Cost { resource_type: ResourceType::DarkOre, amount: 100 }],
                    duration: Duration::from_secs(8),
                }),
            })
            .register_shard(ShardType::Light, ShardInfo {
                name: "Light".to_string(),
                description: "Illuminate the darkness.".to_string(),
                icon: light_shard_image,
                recipe: Some(ShardRecipe {
                    cost: vec![Cost { resource_type: ResourceType::DarkOre, amount: 100 }],
                    duration: Duration::from_secs(8),
                }),
            })
            .register_shard(ShardType::Electric, ShardInfo {
                name: "Electric".to_string(),
                description: "Shock and awe.".to_string(),
                icon: electric_shard_image,
                recipe: Some(ShardRecipe {
                    cost: vec![Cost { resource_type: ResourceType::DarkOre, amount: 100 }],
                    duration: Duration::from_secs(8),
                }),
            })
            ;
    }
}
