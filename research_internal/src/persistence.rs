use std::time::Duration;

use bevy::prelude::*;

use game_core::prelude::MapBound;
use persistence::prelude::SaveableBatchCommand;
use research::{
    model::{
        ActiveResearch, Completed, OutcomeSeed, Research, ResearchCardDisplay,
        ResearchCatalog, ResearchInstantiated, ResearchOutcomeOf, ResearchProgress, ResearchSpec,
    },
    outcomes::GrantShardBlueprint,
    persistence::{BuilderResearch, ResearchSaveData, ShardBlueprintOutcomeSaveData},
};

/// Builds a research instance for both fresh spawns (`save_data == None`) and loads. Fresh spawns
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
    // Born-obsolescence is not handled here — each outcome's `on_grant_shard_blueprint_add_init_outcome` sets `OutcomeSatisfied` if
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

pub(crate) fn save_researches(
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

pub(crate) fn save_shard_blueprint_outcomes(
    mut commands: Commands,
    outcomes: Query<(Entity, &GrantShardBlueprint, &ResearchOutcomeOf)>,
) {
    if outcomes.is_empty() { return; }
    let batch = outcomes.iter().map(|(entity, grant, outcome_of)| {
        ShardBlueprintOutcomeSaveData { entity, research_entity: outcome_of.0, shard_type: grant.0 }
    }).collect::<SaveableBatchCommand<_>>();
    commands.queue(batch);
}
