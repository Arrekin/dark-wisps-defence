use bevy::prelude::*;

use game_core::prelude::{Shard, ShardTier, ShardType};
use persistence::creating_new_map;
use resources::prelude::Stock;
use states::prelude::MapLoadingStage;

/// Seeds a new map's `Stock` with T1 stat shards. Saved maps carry their shards in the stock.
pub(crate) struct StartingShardsPlugin;
impl Plugin for StartingShardsPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(OnEnter(MapLoadingStage::LoadResources), seed_starting_shards.run_if(creating_new_map));
    }
}

// Starting stock of a new map
const STARTING_SHARDS_PER_TYPE: i32 = 10;

fn seed_starting_shards(mut stock: ResMut<Stock>) {
    for shard_type in [ShardType::Strength, ShardType::Speed, ShardType::Reach] {
        stock.add((Shard::new(shard_type, ShardTier::T1), STARTING_SHARDS_PER_TYPE));
    }
}
