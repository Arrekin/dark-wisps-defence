//! Persistence for research instances and their outcomes. Each part owns its own data (no JSON):
//! the research entity saves its scalars + a `research_costs` child table; each outcome kind has its
//! own table. Fresh-spawn and load both flow through `BuilderResearch::on_add` (the Builder pattern).
//!
//! Possession of granted blueprints is saved by the blueprint lanes, not here.

use std::time::Duration;

use crate::lib_prelude::*;

use crate::research::{
    ActiveResearch, Completed, OutcomeSeed, Research, ResearchCardDisplay,
    ResearchCatalog, ResearchInstantiated, ResearchOutcomeOf, ResearchProgress, ResearchSpec,
};
use crate::research_outcomes::GrantShardBlueprint;

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

    pub fn on_add(
        trigger: On<Add, BuilderResearch>,
        mut commands: Commands,
        builders: Query<&BuilderResearch>,
        catalog: Res<ResearchCatalog>,
    ) {
        let entity = trigger.entity;
        let Ok(builder) = builders.get(entity) else { return };
        let research_type = builder.research_type;
        let definition = catalog.get(research_type);

        {
            let mut entity_commands = commands.entity(entity);
            entity_commands.remove::<BuilderResearch>();
            entity_commands.insert((
                Research(research_type),
                MapBound,
                ResearchCardDisplay {
                    title: definition.name.clone(),
                    icon: definition.icon.clone(),
                },
            ));
            match &builder.save_data {
                Some(save_data) => {
                    entity_commands.insert(ResearchSpec {
                        cost: save_data.cost.clone(),
                        duration: Duration::from_secs_f32(save_data.duration_secs),
                    });
                    if save_data.is_completed {
                        entity_commands.insert(Completed);
                    } else if let Some(fraction) = save_data.progress {
                        entity_commands.insert(ResearchProgress { fraction });
                        if save_data.is_active {
                            entity_commands.insert(ActiveResearch);
                        }
                    }
                }
                None => {
                    entity_commands.insert(ResearchSpec {
                        cost: definition.cost.clone(),
                        duration: definition.duration,
                    });
                }
            }
        }

        // Fresh only: compose default outcomes and announce instantiation for modifier systems.
        // Born-obsolescence is not handled here — each outcome's `on_add` sets `OutcomeSatisfied` if
        // already owned, and the generic aggregation marks the research obsolete.
        if builder.save_data.is_none() {
            for outcome_seed in definition.default_outcomes.iter() {
                match outcome_seed {
                    OutcomeSeed::ShardBlueprint(shard_type) => {
                        commands.spawn((GrantShardBlueprint(*shard_type), ResearchOutcomeOf(entity), MapBound));
                    }
                }
            }
            commands.trigger(ResearchInstantiated { research: entity, research_type });
        }
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

pub fn save_researches(
    mut commands: Commands,
    researches: Query<(Entity, &Research, &ResearchSpec, Option<&ResearchProgress>, Has<ActiveResearch>, Has<Completed>)>,
) {
    if researches.is_empty() { return; }
    let batch = researches.iter().map(|(entity, research, spec, progress, is_active, is_completed)| {
        let save_data = ResearchSaveData {
            entity,
            duration_secs: spec.duration.as_secs_f32(),
            cost: spec.cost.clone(),
            progress: progress.map(|p| p.fraction),
            is_active,
            is_completed,
        };
        BuilderResearch::new_for_saving(research.0, save_data)
    }).collect::<SaveableBatchCommand<_>>();
    commands.queue(batch);
}

// ============================================================================
// OUTCOME: shard blueprint grant
// ============================================================================

#[derive(Clone, Debug, SSS)]
struct ShardBlueprintOutcomeSaveData {
    entity: Entity,
    research_entity: Entity,
    shard_type: ShardType,
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

pub fn save_shard_blueprint_outcomes(
    mut commands: Commands,
    outcomes: Query<(Entity, &GrantShardBlueprint, &ResearchOutcomeOf)>,
) {
    if outcomes.is_empty() { return; }
    let batch = outcomes.iter().map(|(entity, grant, outcome_of)| {
        ShardBlueprintOutcomeSaveData { entity, research_entity: outcome_of.0, shard_type: grant.0 }
    }).collect::<SaveableBatchCommand<_>>();
    commands.queue(batch);
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
