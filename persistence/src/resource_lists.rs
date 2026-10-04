//! # Resource lists
//!
//! Universal storage for ordered lists of `ResourceAmount`s. An owner saves its list, keeps the
//! returned id in its own row, and reads the list back by that id. Lists do not know their owners.
//!
//! Each entry is one row in `resource_list_entries` holding its resource kind and amount; kinds
//! that carry data keep it in `resource_list_entry_essences` / `resource_list_entry_shards`, keyed
//! by the entry id.

use game_core::prelude::Shard;
use logging::prelude::*;
use resources::prelude::{ResourceAmount, ResourceType};

use crate::{
    load::{LoadContext, LoadError, LoadResult, LoadRowExtension},
    save::SaveContext,
};

impl SaveContext<'_> {
    /// Writes `entries` as a new list, in order, and returns its id for the owner to store.
    pub fn save_resource_list(&self, entries: &[ResourceAmount]) -> rusqlite::Result<i64> {
        self.tx.prepare_cached("INSERT INTO resource_lists DEFAULT VALUES")?.execute([])?;
        let list_id = self.tx.last_insert_rowid();
        for (position, entry) in entries.iter().enumerate() {
            let kind = match entry.resource_type {
                ResourceType::DarkOre => "DarkOre",
                ResourceType::Essence(_) => "Essence",
                ResourceType::Shard(_) => "Shard",
            };
            self.tx.prepare_cached("INSERT INTO resource_list_entries (list_id, position, kind, amount) VALUES (?1, ?2, ?3, ?4)")?
                .execute(rusqlite::params![list_id, position, kind, entry.amount])?;
            let entry_id = self.tx.last_insert_rowid();
            match entry.resource_type {
                ResourceType::DarkOre => {}
                ResourceType::Essence(essence_type) => {
                    self.tx.prepare_cached("INSERT INTO resource_list_entry_essences (entry_id, essence_type) VALUES (?1, ?2)")?
                        .execute(rusqlite::params![entry_id, essence_type.as_ref()])?;
                }
                ResourceType::Shard(shard) => {
                    self.tx.prepare_cached("INSERT INTO resource_list_entry_shards (entry_id, shard_type, shard_tier) VALUES (?1, ?2, ?3)")?
                        .execute(rusqlite::params![entry_id, shard.shard_type.as_ref(), shard.tier.as_ref()])?;
                }
            }
        }
        Ok(list_id)
    }
}

impl LoadContext<'_> {
    /// Reads list `list_id` in saved order. An entry whose resource the game no longer knows is
    /// logged and skipped; the rest of the list still loads.
    #[log_tags(Tag::GameLoad)]
    pub fn resource_list(&self, list_id: i64) -> LoadResult<Vec<ResourceAmount>> {
        let mut statement = self.conn.prepare_cached(
            "SELECT entry.kind, essence.essence_type, shard.shard_type, shard.shard_tier, entry.amount
             FROM resource_list_entries entry
             LEFT JOIN resource_list_entry_essences essence ON essence.entry_id = entry.id
             LEFT JOIN resource_list_entry_shards shard ON shard.entry_id = entry.id
             WHERE entry.list_id = ?1
             ORDER BY entry.position",
        )?;
        let decode = |row: &rusqlite::Row| -> LoadResult<ResourceType> {
            let kind: String = row.get(0)?;
            match kind.as_str() {
                "DarkOre" => Ok(ResourceType::DarkOre),
                "Essence" => Ok(ResourceType::Essence(row.get_parsed(1)?)),
                "Shard" => Ok(ResourceType::Shard(Shard::new(row.get_parsed(2)?, row.get_parsed(3)?))),
                _ => Err(LoadError::unknown_value("resource kind", kind)),
            }
        };

        let mut rows = statement.query([list_id])?;
        let mut entries = Vec::new();
        while let Some(row) = rows.next()? {
            let amount: i32 = row.get(4)?;
            match decode(row) {
                Ok(resource_type) => entries.push(ResourceAmount::new(resource_type, amount)),
                #[warn_dev("Resource list {list_id} entry skipped: {error}")]
                Err(error) => {}
            }
        }
        Ok(entries)
    }
}
