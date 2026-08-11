use bevy::prelude::*;

use game_core::prelude::ShardType;
use persistence::{
    creating_new_map,
    prelude::{AppGameLoadSaveExtension, CollectSave, LoadContext, SaveWriter},
    rusqlite,
};
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
            .add_systems(OnEnter(MapLoadingStage::LoadResources), seed_starting_shards.run_if(creating_new_map))
            ;
    }
}

fn seed_starting_shards(mut inventory: ResMut<ShardInventory>) {
    inventory.add(ShardType::Range, 10);
    inventory.add(ShardType::Damage, 10);
    inventory.add(ShardType::Speed, 10);
}

fn collect_shard_inventory(inventory: Res<ShardInventory>, mut save: SaveWriter) {
    let rows: Vec<(String, i32)> = inventory
        .iter()
        .map(|(shard_type, count)| (shard_type.to_string(), count as i32))
        .collect();
    if rows.is_empty() { return; }
    save.submit(move |tx| {
        for (shard_type, count) in rows {
            tx.execute(
                "INSERT OR REPLACE INTO shard_inventory (shard_type, count) VALUES (?1, ?2)",
                rusqlite::params![shard_type, count],
            )?;
        }
        Ok(())
    });
}

fn load_shard_inventory(ctx: &mut LoadContext) -> rusqlite::Result<()> {
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

    ctx.insert_resource(inventory);
    Ok(())
}
