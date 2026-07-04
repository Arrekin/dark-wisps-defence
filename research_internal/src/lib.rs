use bevy::prelude::*;
use strum::IntoEnumIterator;

use ::persistence::prelude::{AppGameLoadSaveExtension, CollectSave};
use research::{
    model::{BuilderResearch, Research, ResearchCatalog, ResearchType},
};
use states::prelude::{GameState, MapLoadingStage};

pub(crate) mod process;
pub(crate) mod outcomes;
pub(crate) mod persistence;
pub(crate) mod panel;

pub struct ResearchPlugin;
impl Plugin for ResearchPlugin {
    fn build(&self, app: &mut App) {
        let asset_server = app.world().resource::<AssetServer>();
        let catalog = ResearchCatalog::build(asset_server);
        app
            .insert_resource(catalog)
            .add_observer(persistence::on_builder_add_spawn_research)
            .add_observer(outcomes::on_grant_shard_blueprint_add_init_outcome)
            .add_observer(outcomes::on_shard_blueprint_acquired_mark_outcomes_satisfied)
            .add_observer(process::on_check_for_obsoletion_do_so)
            .add_observer(process::on_insert_outcome_satisfied_recheck_obsoletion)
            .add_observer(process::on_remove_outcome_satisfied_recheck_obsoletion)
            .add_observer(process::on_set_active_research_do_so)
            .add_observer(process::on_stop_research_do_so)
            .add_systems(Update, process::research_tick.run_if(in_state(GameState::Running)))
            .add_systems(OnEnter(MapLoadingStage::Ready), seed_research)
            .add_systems(CollectSave, persistence::collect_researches)
            .add_systems(CollectSave, persistence::collect_shard_blueprint_outcomes)
            .register_loader(MapLoadingStage::SpawnMapElements, "researches", persistence::load_researches)
            .register_loader(MapLoadingStage::SpawnMapElements, "research_outcome_shard_blueprints", persistence::load_shard_blueprint_outcomes)
            ;
        panel::register(app);
    }
}

/// Instantiates any research not already present (fresh map). Loaded maps already have their research
/// entities, so this is a no-op for them. Mirrors `seed_starting_blueprints`.
fn seed_research(
    mut commands: Commands,
    existing: Query<&Research>,
) {
    let present: Vec<ResearchType> = existing.iter().map(|research| research.0).collect();
    for research_type in ResearchType::iter() {
        if !present.contains(&research_type) {
            commands.spawn(BuilderResearch::new(research_type));
        }
    }
}
