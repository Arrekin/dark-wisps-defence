//! # Shard System
//!
//! Shards are player-insertable items that modify entity stats and behaviors.
//! Each entity type defines its own reaction to shard insertion via a per-entity observer
//! registered in its builder.
//!
//! ## Data Model
//! - `ShardSlots` component tracks which shards are socketed into indexed slots.
//! - `ShardApplyEvent` is triggered by `ShardSlots::insert_at` and handled per-entity.
//!
//! ## Persistence
//! `ShardsPlugin` centralizes save and load of all shard slots. Entity builders spawn
//! with empty `ShardSlots`; the shard loader repopulates them after entities spawn.
use crate::lib_prelude::*;
use strum::{Display, EnumString, EnumIter};

pub mod shards_prelude {
    pub use super::{ShardApplyEvent, ShardEffect, ShardSlots, ShardType};
}

pub struct ShardsPlugin;
impl Plugin for ShardsPlugin {
    fn build(&self, app: &mut App) {
        app
            .register_db_saver(ShardSlots::on_game_save)
            .add_systems(OnEnter(MapLoadingStage::SpawnEffectInstances), ShardSlots::load_system)
            ;
    }
}

////////////////////
////  SHARD TYPE  //
////////////////////

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Display, EnumString, EnumIter)]
pub enum ShardType {
    Range,
    Damage,
    Speed,
    Fire,
    Water,
    Light,
    Electric,
}

////////////////////
////  SHARD SLOTS //
////////////////////

/// Fixed-capacity indexed shard slots attached to an entity.
///
/// Call `insert_at` to socket a shard; it writes the slot and triggers `ShardApplyEvent`
/// atomically. Callers without a specific target index should use `first_free_slot` first.
#[derive(Component)]
pub struct ShardSlots {
    slots: Vec<Option<ShardType>>,
}
impl ShardSlots {
    pub fn new(capacity: usize) -> Self {
        Self { slots: vec![None; capacity] }
    }

    /// Sockets a shard into the given slot index, triggering `ShardApplyEvent`.
    /// Panics if `slot_index >= capacity`.
    pub fn insert_at(
        &mut self,
        slot_index: usize,
        shard_type: ShardType,
        shard_target: Entity,
        commands: &mut Commands,
    ) {
        assert!(slot_index < self.slots.len(), "ShardSlots::insert_at: invalid slot index {slot_index}");
        self.slots[slot_index] = Some(shard_type);
        commands.trigger(ShardApplyEvent { shard_target, shard_type });
    }

    /// Returns the index of the first empty slot, or `None` if full.
    pub fn first_free_slot(&self) -> Option<usize> {
        self.slots.iter().position(|s| s.is_none())
    }

    pub fn capacity(&self) -> usize {
        self.slots.len()
    }

    pub fn len(&self) -> usize {
        self.slots.iter().filter(|s| s.is_some()).count()
    }

    pub fn is_full(&self) -> bool {
        self.slots.iter().all(|s| s.is_some())
    }

    pub fn get(&self, slot_index: usize) -> Option<ShardType> {
        self.slots.get(slot_index).and_then(|s| *s)
    }

    /// Iterates occupied slots as `(slot_index, ShardType)`.
    pub fn iter_with_index(&self) -> impl Iterator<Item = (usize, ShardType)> + '_ {
        self.slots.iter().enumerate().filter_map(|(i, s)| s.map(|t| (i, t)))
    }

    /// Iterates shard types of occupied slots in slot-index order.
    pub fn iter(&self) -> impl Iterator<Item = ShardType> + '_ {
        self.slots.iter().filter_map(|s| *s)
    }
}

impl ShardSlots {
    fn on_game_save(
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

    pub fn load_system(
        mut commands: Commands,
        mut shard_slots: Query<&mut ShardSlots>,
        save_executor: Res<GameSaveExecutor>,
        entity_map: Res<DbEntityMap>,
    ) {
        let result = with_db_connection(&save_executor.save_name, |conn| {
            let mut stmt = conn.prepare(
                "SELECT shard_target_id, shard_index, shard_type FROM entity_shards ORDER BY shard_target_id, shard_index"
            )?;
            let mut rows = stmt.query([])?;

            while let Some(row) = rows.next()? {
                let target_id: i64 = row.get(0)?;
                let slot_index: usize = row.get::<_, i32>(1)? as usize;
                let shard_str: String = row.get(2)?;

                let Ok(shard_type) = shard_str.parse::<ShardType>() else { continue };

                let Some(&entity) = entity_map.map.get(&target_id) else {
                    Log::warn().dev().tag(Tag::GameLoad).message(format!("Shard load: no entity mapping for db id {target_id}"));
                    continue
                };
                let Ok(mut slots) = shard_slots.get_mut(entity) else {
                    Log::warn().dev().tag(Tag::GameLoad).message(format!("Shard load: entity {entity:?} has no ShardSlots component"));
                    continue
                };

                slots.insert_at(slot_index, shard_type, entity, &mut commands);
            }
            Ok(())
        });

        if let Err(e) = result {
            Log::error().dev().tag(Tag::GameLoad).message(format!("Shard loading failed: {e}"));
        }
    }
}

#[derive(Clone, Debug, SSS)]
struct ShardSlotSaveData {
    entity: Entity,
    slot_index: usize,
    shard_type: ShardType,
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

////////////////////////
////  SHARD EFFECT    //
////////////////////////

/// Marker on effect entities spawned by shard application.
///
/// Distinguishes shard effects from baseline effects, aura effects, debuffs, etc.
/// Used for removal queries, save/load filtering, and UI.
#[derive(Component)]
pub struct ShardEffect;
impl ShardEffect {
    /// Returns a bundle for a stat-only shard effect. Convenience for the common case
    /// where a shard simply contributes modifier values with no behavioral markers.
    pub fn from_modifiers(shard_target: Entity, contributions: HashMap<ModifierType, f32>) -> impl Bundle {
        (EffectTarget(shard_target), ModifierContributions(contributions), ShardEffect)
    }
}

///////////////////////
////  SHARD EVENTS   //
///////////////////////

/// Triggered by `ShardSlots::insert_at` when a shard is socketed.
///
/// Each entity type registers a per-entity observer for this event in its builder's `on_add`.
/// The observer is responsible for spawning the appropriate effect entity.
#[derive(EntityEvent, Clone, Copy)]
pub struct ShardApplyEvent {
    #[event_target]
    pub shard_target: Entity,
    pub shard_type: ShardType,
}
