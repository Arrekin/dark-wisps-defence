use bevy::prelude::*;

use game_core::prelude::{MapBound, ShardType, SSS};
use logging::prelude::{Log, Tag};
use persistence::{
    prelude::{GameDbHelpers, Loadable, LoadContext, LoadResult, Saveable},
    rusqlite,
};
use resources::prelude::{Cost, EssenceType, ResourceType};

use crate::{
    model::{ResearchOutcomeOf, ResearchType},
    outcomes::GrantShardBlueprint,
};

// ============================================================================
// RESEARCH ENTITY
// ============================================================================

#[derive(Clone, Debug)]
pub struct ResearchSaveData {
    pub entity: Entity,
    pub duration_secs: f32,
    pub cost: Vec<Cost>,
    /// `Some(fraction)` while in flight; `None` when not started or completed.
    pub progress: Option<f32>,
    pub is_active: bool,
    pub is_completed: bool,
}

/// Builds a research instance for both fresh spawns (`save_data == None`) and loads. Fresh spawns
/// clone the definition, spawn default outcomes, and fire `ResearchInstantiated`; loads restore the
/// saved scalars and never re-fire (the saved composition, including modifier-added outcomes, is
/// authoritative).
#[derive(Component, SSS)]
pub struct BuilderResearch {
    pub research_type: ResearchType,
    pub save_data: Option<ResearchSaveData>,
}
impl BuilderResearch {
    pub fn new(research_type: ResearchType) -> Self {
        Self { research_type, save_data: None }
    }
    pub fn new_for_saving(research_type: ResearchType, save_data: ResearchSaveData) -> Self {
        Self { research_type, save_data: Some(save_data) }
    }
}
impl Saveable for BuilderResearch {
    fn save(self, tx: &rusqlite::Transaction) -> rusqlite::Result<()> {
        let save_data = self.save_data.expect("BuilderResearch for saving must have save_data");
        let id = save_data.entity.index_u32() as i64;
        tx.register_entity(id)?;
        tx.execute(
            "INSERT OR REPLACE INTO researches (id, research_type, duration_secs, progress, is_active, is_completed) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            rusqlite::params![
                id,
                self.research_type.to_string(),
                save_data.duration_secs,
                save_data.progress,
                save_data.is_active as i32,
                save_data.is_completed as i32,
            ],
        )?;
        for cost in save_data.cost.iter() {
            let (resource_kind, essence_type) = encode_resource(cost.resource_type);
            tx.execute(
                "INSERT INTO research_costs (research_id, resource_kind, essence_type, amount) VALUES (?1, ?2, ?3, ?4)",
                rusqlite::params![id, resource_kind, essence_type, cost.amount],
            )?;
        }
        Ok(())
    }
}
impl Loadable for BuilderResearch {
    fn load(ctx: &mut LoadContext) -> rusqlite::Result<LoadResult> {
        let mut stmt = ctx.conn.prepare("SELECT id, research_type, duration_secs, progress, is_active, is_completed FROM researches LIMIT ?1 OFFSET ?2")?;
        let mut rows = stmt.query(ctx.pagination.as_params())?;

        let mut count = 0;
        while let Some(row) = rows.next()? {
            let old_id: i64 = row.get(0)?;
            let type_str: String = row.get(1)?;
            let duration_secs: f32 = row.get(2)?;
            let progress_value: Option<f32> = row.get(3)?;
            let is_active: bool = row.get::<_, i32>(4)? != 0;
            let is_completed: bool = row.get::<_, i32>(5)? != 0;
            count += 1;

            let Ok(research_type) = type_str.parse::<ResearchType>() else {
                Log::warn().dev().tag(Tag::GameLoad).message(format!("Unknown research type in save: {type_str}"));
                continue;
            };
            let cost = read_costs(ctx.conn, old_id)?;

            let Some(new_entity) = ctx.get_new_entity_for_old(old_id) else {
                Log::warn().dev().tag(Tag::GameLoad).message(format!("Research with old ID {old_id} has no new entity"));
                continue;
            };
            let progress = if is_completed { None } else { progress_value };
            let save_data = ResearchSaveData { entity: new_entity, duration_secs, cost, progress, is_active, is_completed };
            ctx.commands.entity(new_entity).insert(BuilderResearch::new_for_saving(research_type, save_data));
        }
        Ok(count.into())
    }
}

fn read_costs(conn: &rusqlite::Connection, research_id: i64) -> rusqlite::Result<Vec<Cost>> {
    let mut stmt = conn.prepare("SELECT resource_kind, essence_type, amount FROM research_costs WHERE research_id = ?1")?;
    let mut rows = stmt.query([research_id])?;
    let mut cost = Vec::new();
    while let Some(row) = rows.next()? {
        let resource_kind: String = row.get(0)?;
        let essence_type: Option<String> = row.get(1)?;
        let amount: i32 = row.get(2)?;
        if let Some(resource_type) = decode_resource(&resource_kind, essence_type) {
            cost.push(Cost { resource_type, amount });
        } else {
            Log::warn().dev().tag(Tag::GameLoad).message(format!("Undecodable research cost resource kind: {resource_kind}"));
        }
    }
    Ok(cost)
}

// ============================================================================
// OUTCOME: shard blueprint grant
// ============================================================================

#[derive(Clone, Debug, SSS)]
pub struct ShardBlueprintOutcomeSaveData {
    pub entity: Entity,
    pub research_entity: Entity,
    pub shard_type: ShardType,
}
impl Saveable for ShardBlueprintOutcomeSaveData {
    fn save(self, tx: &rusqlite::Transaction) -> rusqlite::Result<()> {
        let id = self.entity.index_u32() as i64;
        let research_id = self.research_entity.index_u32() as i64;
        tx.register_entity(id)?;
        tx.execute(
            "INSERT OR REPLACE INTO research_outcome_shard_blueprints (id, research_id, shard_type) VALUES (?1, ?2, ?3)",
            rusqlite::params![id, research_id, self.shard_type.to_string()],
        )?;
        Ok(())
    }
}

pub struct ShardBlueprintOutcomeLoader;
impl Loadable for ShardBlueprintOutcomeLoader {
    fn load(ctx: &mut LoadContext) -> rusqlite::Result<LoadResult> {
        let mut stmt = ctx.conn.prepare("SELECT id, research_id, shard_type FROM research_outcome_shard_blueprints LIMIT ?1 OFFSET ?2")?;
        let mut rows = stmt.query(ctx.pagination.as_params())?;

        let mut count = 0;
        while let Some(row) = rows.next()? {
            let old_id: i64 = row.get(0)?;
            let research_old_id: i64 = row.get(1)?;
            let shard_str: String = row.get(2)?;
            count += 1;

            let Ok(shard_type) = shard_str.parse::<ShardType>() else {
                Log::warn().dev().tag(Tag::GameLoad).message(format!("Unknown shard type in saved research outcome: {shard_str}"));
                continue;
            };
            let (Some(outcome_entity), Some(research_entity)) =
                (ctx.get_new_entity_for_old(old_id), ctx.get_new_entity_for_old(research_old_id))
            else {
                Log::warn().dev().tag(Tag::GameLoad).message(format!("Shard-blueprint outcome {old_id} has no mapped entity"));
                continue;
            };
            ctx.commands.entity(outcome_entity).insert((
                GrantShardBlueprint(shard_type),
                ResearchOutcomeOf(research_entity),
                MapBound,
            ));
        }
        Ok(count.into())
    }
}

// ============================================================================
// RESOURCE ENCODING (no JSON)
// ============================================================================

fn encode_resource(resource_type: ResourceType) -> (String, Option<String>) {
    match resource_type {
        ResourceType::DarkOre => ("DarkOre".to_string(), None),
        ResourceType::Essence(essence) => ("Essence".to_string(), Some(essence.as_ref().to_string())),
    }
}

fn decode_resource(resource_kind: &str, essence_type: Option<String>) -> Option<ResourceType> {
    match resource_kind {
        "DarkOre" => Some(ResourceType::DarkOre),
        "Essence" => essence_type.and_then(|name| name.parse::<EssenceType>().ok()).map(ResourceType::Essence),
        _ => None,
    }
}
