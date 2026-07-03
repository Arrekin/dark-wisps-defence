use bevy::prelude::*;

use game_core::prelude::{ShardType, SSS};
use persistence::{
    prelude::{AppGameLoadSaveExtension, Loadable, LoadContext, LoadResult, Saveable, SaveableBatchCommand},
    rusqlite,
};
use shards::inventory::ShardInventory;
use states::prelude::MapLoadingStage;

pub struct ShardInventoryPlugin;
impl Plugin for ShardInventoryPlugin {
    fn build(&self, app: &mut App) {
        app
            .init_resource::<ShardInventory>()
            .register_db_saver(save_shard_inventory_on_game_save)
            .register_db_loader::<ShardInventoryLoader>(MapLoadingStage::LoadResources)
            .add_systems(OnEnter(MapLoadingStage::Ready), seed_starting_shards)
            ;
    }
}

fn seed_starting_shards(mut inventory: ResMut<ShardInventory>) {
    inventory.add(ShardType::Range, 10);
    inventory.add(ShardType::Damage, 10);
    inventory.add(ShardType::Speed, 10);
}

fn save_shard_inventory_on_game_save(mut commands: Commands, inventory: Res<ShardInventory>) {
    let batch = inventory
        .iter()
        .map(|(shard_type, count)| ShardInventorySaveData { shard_type, count })
        .collect::<SaveableBatchCommand<_>>();
    commands.queue(batch);
}

#[derive(Clone, Debug, SSS)]
struct ShardInventorySaveData {
    shard_type: ShardType,
    count: usize,
}

impl Saveable for ShardInventorySaveData {
    fn save(self, tx: &rusqlite::Transaction) -> rusqlite::Result<()> {
        tx.execute(
            "INSERT OR REPLACE INTO shard_inventory (shard_type, count) VALUES (?1, ?2)",
            rusqlite::params![self.shard_type.to_string(), self.count as i32],
        )?;
        Ok(())
    }
}

struct ShardInventoryLoader;
impl Loadable for ShardInventoryLoader {
    fn load(ctx: &mut LoadContext) -> rusqlite::Result<LoadResult> {
        let mut stmt = ctx.conn.prepare("SELECT shard_type, count FROM shard_inventory")?;
        let mut rows = stmt.query([])?;

        let mut inventory = ShardInventory::default();
        while let Some(row) = rows.next()? {
            let shard_str: String = row.get(0)?;
            let count: i32 = row.get(1)?;
            if let Ok(shard_type) = shard_str.parse::<ShardType>() {
                inventory.add(shard_type, count as usize);
            }
        }

        ctx.commands.insert_resource(inventory);
        Ok(LoadResult::Finished)
    }
}
