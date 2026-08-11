use bevy::prelude::*;
use almanach::prelude::{AlmanachAppExt, ResearchSpawnFn};
use persistence::{creating_new_map, prelude::{AppGameLoadSaveExtension, CollectSave}};
use research::prelude::SeedResearches;
use states::prelude::{GameState, MapLoadingStage};

use definitions::shards::*;

mod core;
mod definitions;
mod process;
mod save_load;
mod ui;

pub struct ResearchPlugin;
impl Plugin for ResearchPlugin {
    fn build(&self, app: &mut App) {
        let definitions: [(&str, ResearchSpawnFn); 4] = [
            ("fire_shard_recipe_research",     spawn_fire_shard_recipe_research),
            ("water_shard_recipe_research",    spawn_water_shard_recipe_research),
            ("light_shard_recipe_research",    spawn_light_shard_recipe_research),
            ("electric_shard_recipe_research", spawn_electric_shard_recipe_research),
        ];
        for (id, spawn_fn) in definitions {
            app.register_research(id, spawn_fn);
        }

        app
            .add_observer(core::on_add_research_state_spawn_tile)
            .add_observer(core::on_insert_research_state_sync_markers)
            .add_observer(core::on_insert_display_icon_fire_research_display_data_updated)
            .add_observer(core::on_seed_researches_spawn_missing)
            .add_observer(process::on_set_active_research)
            .add_observer(process::on_stop_research)
            .add_observer(process::on_research_finished)
            .add_systems(Update, process::research_tick.run_if(in_state(GameState::Running)))
            .add_systems(CollectSave, save_load::collect_researches)
            .register_loader(MapLoadingStage::SpawnMapElements, "researches", save_load::load_researches)
            // New maps need every catalog research available for scenario authoring.
            .add_systems(OnEnter(MapLoadingStage::SpawnMapElements), (|mut commands: Commands| { commands.trigger(SeedResearches); }).run_if(creating_new_map))
            .add_plugins(ui::ResearchUiPlugin);
    }
}
