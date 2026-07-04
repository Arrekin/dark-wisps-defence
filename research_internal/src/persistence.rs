use std::time::Duration;

use bevy::prelude::*;

use game_core::prelude::{MapBound, ShardType};
use logging::prelude::*;
use ::persistence::{
    prelude::{GameDbHelpers, LoadContext, SaveWriter},
    rusqlite,
};
use research::{
    model::{
        ActiveResearch, BuilderResearch, Completed, OutcomeSeed, Research, ResearchCardDisplay,
        ResearchCatalog, ResearchInstantiated, ResearchOutcomeOf, ResearchProgress, ResearchSpec,
        ResearchType,
    },
    outcomes::GrantShardBlueprint,
};
use resources::prelude::{Cost, EssenceType, ResourceType};

/// Builds a research instance for both fresh spawns and loads. Fresh spawns
/// clone the definition, spawn default outcomes, and fire `ResearchInstantiated`; loads restore the
/// saved scalars and never re-fire (the saved composition, including modifier-added outcomes, is
/// authoritative).
pub(crate) fn on_builder_add_spawn_research(
    trigger: On<Add, BuilderResearch>,
    mut commands: Commands,
    catalog: Res<ResearchCatalog>,
    builders: Query<&BuilderResearch>,
) {
    let entity = trigger.entity;
    let Ok(builder) = builders.get(entity) else { return };
    let research_type = builder.research_type;
    let definition = catalog.get(research_type);
    let is_fresh = builder.cost.is_none();

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
        if let Some(cost) = builder.cost.clone() {
            let duration_secs = builder.duration_secs.expect("duration_secs set whenever cost is");
            entity_commands.insert(ResearchSpec {
                cost,
                duration: Duration::from_secs_f32(duration_secs),
            });
            if builder.is_completed {
                entity_commands.insert(Completed);
            } else if let Some(fraction) = builder.progress {
                entity_commands.insert(ResearchProgress { fraction });
                if builder.is_active {
                    entity_commands.insert(ActiveResearch);
                }
            }
        } else {
            entity_commands.insert(ResearchSpec {
                cost: definition.cost.clone(),
                duration: definition.duration,
            });
        }
    }

    // Fresh only: compose default outcomes and announce instantiation for modifier systems.
    // Born-obsolescence is not handled here — each outcome's `on_grant_shard_blueprint_add_init_outcome` sets `OutcomeSatisfied` if
    // already owned, and the generic aggregation marks the research obsolete.
    if is_fresh {
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

// ============================================================================
// COLLECTORS (save)
// ============================================================================

pub(crate) fn collect_researches(
    researches: Query<(Entity, &Research, &ResearchSpec, Option<&ResearchProgress>, Has<ActiveResearch>, Has<Completed>)>,
    mut save: SaveWriter,
) {
    if researches.is_empty() { return; }
    let rows: Vec<(i64, String, f32, Option<f32>, bool, bool, Vec<(String, Option<String>, i32)>)> = researches
        .iter()
        .map(|(entity, research, spec, progress, is_active, is_completed)| {
            let costs: Vec<(String, Option<String>, i32)> = spec.cost.iter().map(|cost| {
                let (resource_kind, essence_type) = encode_resource(cost.resource_type);
                (resource_kind, essence_type, cost.amount)
            }).collect();
            (
                entity.index_u32() as i64,
                research.0.to_string(),
                spec.duration.as_secs_f32(),
                progress.map(|p| p.fraction),
                is_active,
                is_completed,
                costs,
            )
        })
        .collect();
    Log::debug().dev().tag(Tag::GameSave).message(format!("Saving {} researches", rows.len()));
    save.submit(move |tx| {
        for (id, research_type, duration_secs, progress, is_active, is_completed, costs) in rows {
            tx.register_entity(id)?;
            tx.execute(
                "INSERT OR REPLACE INTO researches (id, research_type, duration_secs, progress, is_active, is_completed) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                rusqlite::params![
                    id,
                    research_type,
                    duration_secs,
                    progress,
                    is_active as i32,
                    is_completed as i32,
                ],
            )?;
            for (resource_kind, essence_type, amount) in costs {
                tx.execute(
                    "INSERT INTO research_costs (research_id, resource_kind, essence_type, amount) VALUES (?1, ?2, ?3, ?4)",
                    rusqlite::params![id, resource_kind, essence_type, amount],
                )?;
            }
        }
        Ok(())
    });
}

pub(crate) fn collect_shard_blueprint_outcomes(
    outcomes: Query<(Entity, &GrantShardBlueprint, &ResearchOutcomeOf)>,
    mut save: SaveWriter,
) {
    if outcomes.is_empty() { return; }
    let rows: Vec<(i64, i64, String)> = outcomes
        .iter()
        .map(|(entity, grant, outcome_of)| {
            (
                entity.index_u32() as i64,
                outcome_of.0.index_u32() as i64,
                grant.0.to_string(),
            )
        })
        .collect();
    Log::debug().dev().tag(Tag::GameSave).message(format!("Saving {} shard-blueprint outcomes", rows.len()));
    save.submit(move |tx| {
        for (id, research_id, shard_type) in rows {
            tx.register_entity(id)?;
            tx.execute(
                "INSERT OR REPLACE INTO research_outcome_shard_blueprints (id, research_id, shard_type) VALUES (?1, ?2, ?3)",
                rusqlite::params![id, research_id, shard_type],
            )?;
        }
        Ok(())
    });
}

// ============================================================================
// LOADERS (load)
// ============================================================================

pub(crate) fn load_researches(ctx: &mut LoadContext) -> rusqlite::Result<()> {
    let mut stmt = ctx.conn.prepare("SELECT id, research_type, duration_secs, progress, is_active, is_completed FROM researches")?;
    let mut rows = stmt.query([])?;
    while let Some(row) = rows.next()? {
        let old_id: i64 = row.get(0)?;
        let type_str: String = row.get(1)?;
        let duration_secs: f32 = row.get(2)?;
        let progress_value: Option<f32> = row.get(3)?;
        let is_active: bool = row.get::<_, i32>(4)? != 0;
        let is_completed: bool = row.get::<_, i32>(5)? != 0;

        let Ok(research_type) = type_str.parse::<ResearchType>() else {
            Log::warn().dev().tag(Tag::GameLoad).message(format!("Unknown research type in save: {type_str}"));
            continue;
        };
        let cost = read_costs(ctx.conn, old_id)?;

        let Some(entity) = ctx.entity(old_id) else {
            Log::warn().dev().tag(Tag::GameLoad).message(format!("Research with old ID {old_id} has no new entity"));
            continue;
        };
        let progress = if is_completed { None } else { progress_value };
        let mut builder = BuilderResearch::new(research_type)
            .with_duration_secs(duration_secs)
            .with_cost(cost);
        if let Some(fraction) = progress {
            builder = builder.with_progress(fraction);
        }
        if is_active {
            builder = builder.with_active();
        }
        if is_completed {
            builder = builder.with_completed();
        }
        ctx.insert(entity, builder);
    }
    Ok(())
}

pub(crate) fn load_shard_blueprint_outcomes(ctx: &mut LoadContext) -> rusqlite::Result<()> {
    let mut stmt = ctx.conn.prepare("SELECT id, research_id, shard_type FROM research_outcome_shard_blueprints")?;
    let mut rows = stmt.query([])?;
    while let Some(row) = rows.next()? {
        let old_id: i64 = row.get(0)?;
        let research_old_id: i64 = row.get(1)?;
        let shard_str: String = row.get(2)?;

        let Ok(shard_type) = shard_str.parse::<ShardType>() else {
            Log::warn().dev().tag(Tag::GameLoad).message(format!("Unknown shard type in saved research outcome: {shard_str}"));
            continue;
        };
        let (Some(outcome_entity), Some(research_entity)) =
            (ctx.entity(old_id), ctx.entity(research_old_id))
        else {
            Log::warn().dev().tag(Tag::GameLoad).message(format!("Shard-blueprint outcome {old_id} has no mapped entity"));
            continue;
        };
        ctx.insert(outcome_entity, (
            GrantShardBlueprint(shard_type),
            ResearchOutcomeOf(research_entity),
            MapBound,
        ));
    }
    Ok(())
}

// ============================================================================
// RESOURCE ENCODING (no JSON)
// ============================================================================

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
