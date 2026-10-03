use bevy::prelude::*;

use game_core::prelude::ShardType;
use persistence::{creating_new_map, prelude::*, rusqlite};
use shards::inventory::ShardInventory;
use states::prelude::MapLoadingStage;

pub struct ShardInventoryPlugin;
impl Plugin for ShardInventoryPlugin {
    fn build(&self, app: &mut App) {
        app
            .init_resource::<ShardInventory>()
            .add_systems(OnEnter(MapLoadingStage::Init), |mut commands: Commands| { commands.insert_resource(ShardInventory::default()); })
            .add_systems(CollectSave, collect_shard_inventory)
            .register_loader(MapLoadingStage::LoadResources, "shard_inventory", load_shard_inventory)
            .add_systems(OnEnter(MapLoadingStage::LoadResources), seed_starting_shards.run_if(creating_new_map));
    }
}

fn seed_starting_shards(mut inventory: ResMut<ShardInventory>) {
    inventory.add(ShardType::Range, 10);
    inventory.add(ShardType::Damage, 10);
    inventory.add(ShardType::Speed, 10);
}

fn collect_shard_inventory(inventory: Res<ShardInventory>, mut save: SaveWriter) {
    let shard_inventory = inventory.clone();
    save.submit(move |ctx| {
        for (shard_type, count) in shard_inventory.iter() {
            ctx.tx.execute(
                "INSERT OR REPLACE INTO shard_inventory (shard_type, count) VALUES (?1, ?2)",
                rusqlite::params![shard_type.as_ref(), count],
            )?;
        }
        Ok(())
    });
}

fn load_shard_inventory(ctx: &mut LoadContext) -> LoadResult {
    let mut inventory = ShardInventory::default();
    ctx.for_each_row("SELECT shard_type, count FROM shard_inventory", |_, row| {
        let shard_type = row.get_parsed(0)?;
        let count: usize = row.get(1)?;
        inventory.add(shard_type, count);
        Ok(())
    })?;
    ctx.insert_resource(inventory);
    Ok(())
}
