use bevy::prelude::*;

use game_core::prelude::{ShardType, SSS};
use logging::prelude::*;
use persistence::{prelude::*, rusqlite};
use shards::prelude::ShardSlots;
use states::prelude::MapLoadingStage;

pub struct ShardSlotsPlugin;
impl Plugin for ShardSlotsPlugin {
    fn build(&self, app: &mut App) {
        app
            .add_observer(on_builder_add_populate_shard_slots)
            .add_systems(CollectSave, collect_shard_slots)
            .register_loader(MapLoadingStage::SpawnEffectInstances, "entity_shards", load_shard_slots);
    }
}

/// Save/load unit for a single shard slot assignment.
///
/// `ShardSlots` is a container component holding N slots, but persistence saves each slot
/// as a separate row in `entity_shards`. This breaks the standard builder pattern (one
/// component → one row → one entity spawn) because:
/// - Save: `collect_shard_slots` produces N rows (one per occupied slot).
/// - Load: the loader can only insert components via `LoadContext`, not mutate existing
///   ones. So on load, `BuilderShardSlot` is inserted as a throwaway component on the existing
///   entity; `on_builder_add_populate_shard_slots` copies the slot data into `ShardSlots` via `insert_at` and removes itself.
///
/// This is a workaround for a framework limitation (no one-to-many component persistence) and for
/// the fact that `ShardSlots` is slated for a full rework. Not a pattern to replicate elsewhere.
#[derive(Component, Clone, Copy, Debug, SSS)]
pub(crate) struct BuilderShardSlot {
    pub slot_index: usize,
    pub shard_type: ShardType,
}
impl BuilderShardSlot {
    pub fn new(slot_index: usize, shard_type: ShardType) -> Self {
        Self { slot_index, shard_type }
    }
}

#[log_tags(Tag::GameLoad)]
fn on_builder_add_populate_shard_slots(
    trigger: On<Add, BuilderShardSlot>,
    mut commands: Commands,
    builders: Query<&BuilderShardSlot>,
    mut shard_slots: Query<&mut ShardSlots>,
) {
    let entity = trigger.entity;
    let Ok(builder) = builders.get(entity) else { return; };
    #[warn_dev("Entity {entity} has no ShardSlots component to load its shard into")]
    let Ok(mut slots) = shard_slots.get_mut(entity) else {
        commands.entity(entity).remove::<BuilderShardSlot>();
        return;
    };
    slots.insert_at(builder.slot_index, builder.shard_type, entity, &mut commands);
    commands.entity(entity).remove::<BuilderShardSlot>();
}

#[log_tags(Tag::GameSave)]
fn collect_shard_slots(
    shard_targets: Query<(Entity, &ShardSlots)>,
    mut save: SaveWriter,
) {
    #[debug_dev("Saving {} shard slots", rows.len())]
    let rows: Vec<(i64, usize, ShardType)> = shard_targets.iter()
        .flat_map(|(entity, slots)| {
            slots.iter_with_index().map(move |(slot_index, shard_type)| {
                (
                    entity.index_u32() as i64,
                    slot_index,
                    shard_type,
                )
            })
        })
        .collect();
    if rows.is_empty() { return; }
    save.submit(move |ctx| {
        for (entity_id, slot_index, shard_type) in rows {
            ctx.register_entity(entity_id)?;
            ctx.tx.execute(
                "INSERT INTO entity_shards (shard_target_id, shard_index, shard_type) VALUES (?1, ?2, ?3)",
                rusqlite::params![entity_id, slot_index, shard_type.as_ref()],
            )?;
        }
        Ok(())
    });
}

fn load_shard_slots(ctx: &mut LoadContext) -> LoadResult {
    ctx.for_each_entity("SELECT shard_target_id, shard_index, shard_type FROM entity_shards ORDER BY shard_target_id, shard_index", |ctx, _, entity, row| {
        let slot_index: usize = row.get(1)?;
        let shard_type = row.get_parsed::<ShardType>(2)?;
        ctx.insert(entity, BuilderShardSlot::new(slot_index, shard_type));
        Ok(())
    })
}
