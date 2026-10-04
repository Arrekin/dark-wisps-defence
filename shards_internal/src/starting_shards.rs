use bevy::prelude::*;
use strum::IntoEnumIterator;

use game_core::prelude::{Shard, ShardTier, ShardType};
use persistence::creating_new_map;
use resources::prelude::Stock;
use states::prelude::MapLoadingStage;

/// Seeds a new map's `Stock` with stat shards of every tier. Saved maps carry their shards in the stock.
pub(crate) struct StartingShardsPlugin;
impl Plugin for StartingShardsPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(OnEnter(MapLoadingStage::LoadResources), seed_starting_shards.run_if(creating_new_map));
    }
}

// Starting stock of a new map
const STARTING_SHARDS_PER_KIND: i32 = 10;

fn seed_starting_shards(mut stock: ResMut<Stock>) {
    for shard_type in [ShardType::Strength, ShardType::Speed, ShardType::Reach] {
        for tier in ShardTier::iter() {
            stock.add((Shard::new(shard_type, tier), STARTING_SHARDS_PER_KIND));
        }
    }
}
