use bevy::prelude::*;

use game_core::prelude::{ShardType, SSS};
use logging::prelude::{Log, Tag};
use persistence::{
    prelude::{AppGameLoadSaveExtension, GameDbHelpers, Loadable, LoadContext, LoadResult, Saveable, SaveableBatchCommand},
    rusqlite,
};
use shards::prelude::ShardSlots;
use states::prelude::MapLoadingStage;

pub struct ShardSlotsPlugin;
impl Plugin for ShardSlotsPlugin {
    fn build(&self, app: &mut App) {
        app
            .add_observer(populate_shard_slots_on_add)
            .register_db_saver(save_shard_slots_on_game_save)
            .register_db_loader::<ShardSlotSaveData>(MapLoadingStage::SpawnEffectInstances)
            ;
    }
}

/// Save/load unit for a single shard slot assignment.
///
/// `ShardSlots` is a container component holding N slots, but persistence saves each slot
/// as a separate row in `entity_shards`. This breaks the standard builder pattern (one
/// component → one row → one entity spawn) because:
/// - Save: `save_shard_slots_on_game_save` produces N `ShardSlotSaveData` items (one per occupied slot).
/// - Load: the `Loadable` trait can only insert components via `ctx.commands`, not mutate existing
///   ones. So on load, `ShardSlotSaveData` is inserted as a throwaway component on the existing
///   entity; `populate_shard_slots_on_add` copies the slot data into `ShardSlots` via `insert_at` and removes itself.
///
/// This is a workaround for a framework limitation (no one-to-many component persistence) and for
/// the fact that `ShardSlots` is slated for a full rework. Not a pattern to replicate elsewhere.
#[derive(Component, Clone, Copy, Debug, SSS)]
pub(crate) struct ShardSlotSaveData {
    pub entity: Entity,
    pub slot_index: usize,
    pub shard_type: ShardType,
}
impl Saveable for ShardSlotSaveData {
    fn save(self, tx: &rusqlite::Transaction) -> rusqlite::Result<()> {
        let entity_id = self.entity.index_u32() as i64;
        tx.register_entity(entity_id)?;
        tx.execute(
            "INSERT INTO entity_shards (shard_target_id, shard_index, shard_type) VALUES (?1, ?2, ?3)",
            rusqlite::params![entity_id, self.slot_index as i32, self.shard_type.to_string()],
        )?;
        Ok(())
    }
}
impl Loadable for ShardSlotSaveData {
    fn load(ctx: &mut LoadContext) -> rusqlite::Result<LoadResult> {
        let mut stmt = ctx.conn.prepare(
            "SELECT shard_target_id, shard_index, shard_type FROM entity_shards ORDER BY shard_target_id, shard_index LIMIT ?1 OFFSET ?2"
        )?;
        let mut rows = stmt.query(ctx.pagination.as_params())?;

        let mut count = 0;
        while let Some(row) = rows.next()? {
            let target_id: i64 = row.get(0)?;
            let slot_index: usize = row.get::<_, i32>(1)? as usize;
            let shard_str: String = row.get(2)?;

            let Ok(shard_type) = shard_str.parse::<ShardType>() else { continue };

            let Some(entity) = ctx.get_new_entity_for_old(target_id) else {
                Log::warn().dev().tag(Tag::GameLoad).message(format!("Shard load: no entity mapping for db id {target_id}"));
                continue;
            };

            ctx.commands.entity(entity).insert(ShardSlotSaveData { entity, slot_index, shard_type });
            count += 1;
        }
        Ok(count.into())
    }
}

fn populate_shard_slots_on_add(
    trigger: On<Add, ShardSlotSaveData>,
    mut commands: Commands,
    builders: Query<&ShardSlotSaveData>,
    mut shard_slots: Query<&mut ShardSlots>,
) {
    let entity = trigger.entity;
    let Ok(builder) = builders.get(entity) else { return; };
    let Ok(mut slots) = shard_slots.get_mut(entity) else {
        Log::warn().dev().tag(Tag::GameLoad).message(format!("Shard load: entity {entity:?} has no ShardSlots component"));
        commands.entity(entity).remove::<ShardSlotSaveData>();
        return;
    };
    slots.insert_at(builder.slot_index, builder.shard_type, entity, &mut commands);
    commands.entity(entity).remove::<ShardSlotSaveData>();
}

fn save_shard_slots_on_game_save(
    mut commands: Commands,
    shard_targets: Query<(Entity, &ShardSlots)>,
) {
    let batch = shard_targets.iter()
        .flat_map(|(entity, slots)| {
            slots.iter_with_index().map(move |(slot_index, shard_type)| {
                ShardSlotSaveData { entity, slot_index, shard_type }
            })
        })
        .collect::<SaveableBatchCommand<_>>();
    commands.queue(batch);
}
