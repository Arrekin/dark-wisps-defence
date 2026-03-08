//! # Shard Inventory
//!
//! Tracks how many of each shard type the player currently holds.
//! Shards are consumed on insertion and returned on removal.

use crate::lib_prelude::*;

pub mod shard_inventory_prelude {
    pub use super::ShardInventory;
}

pub struct ShardInventoryPlugin;
impl Plugin for ShardInventoryPlugin {
    fn build(&self, app: &mut App) {
        app
            .init_resource::<ShardInventory>()
            .add_systems(Startup, ShardInventory::seed_starting_shards)
            .register_db_saver(ShardInventory::on_game_save)
            .register_db_loader::<ShardInventoryLoader>(MapLoadingStage::LoadResources)
            ;
    }
}

#[derive(Resource, Default)]
pub struct ShardInventory {
    shards: HashMap<ShardType, usize>,
}
impl ShardInventory {
    pub fn count(&self, shard_type: ShardType) -> usize {
        self.shards.get(&shard_type).copied().unwrap_or(0)
    }

    pub fn has(&self, shard_type: ShardType) -> bool {
        self.count(shard_type) > 0
    }

    pub fn add(&mut self, shard_type: ShardType, amount: usize) {
        *self.shards.entry(shard_type).or_insert(0) += amount;
    }

    /// Removes one shard of the given type.
    ///
    /// Caller must verify `has(shard_type)` before calling. Panics if count is zero.
    pub fn remove(&mut self, shard_type: ShardType) {
        let count = self.shards.get_mut(&shard_type)
            .expect("ShardInventory::remove called for absent shard type");
        assert!(*count > 0, "ShardInventory::remove called with zero count");
        *count -= 1;
    }

    pub fn iter(&self) -> impl Iterator<Item = (ShardType, usize)> + '_ {
        self.shards.iter().map(|(&shard_type, &count)| (shard_type, count))
    }

    fn seed_starting_shards(mut inventory: ResMut<ShardInventory>) {
        inventory.add(ShardType::Range, 20);
    }

    fn on_game_save(mut commands: Commands, inventory: Res<ShardInventory>) {
        let batch = inventory
            .iter()
            .map(|(shard_type, count)| ShardInventorySaveData { shard_type, count })
            .collect::<SaveableBatchCommand<_>>();
        commands.queue(batch);
    }
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
