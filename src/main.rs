mod buildings;
mod editor;
mod map_objects;
mod objectives;
mod overlays;
mod prelude;
mod research;
mod ui;
mod units;
mod visual_effects;
mod weaponry;
mod wisps;

use bevy::ecs::message::Messages;
use crate::prelude::*;

fn main() {
    App::new()
        .insert_resource(ClearColor(Color::srgb_u8(30, 31, 34)))
        .add_plugins((
            DefaultPlugins
                .set(ImagePlugin::default_nearest())
                // Warning: VSync is causing a lot of issues with mouse events processing
                .set(WindowPlugin{ primary_window: Some(Window { present_mode: bevy::window::PresentMode::AutoNoVsync, ..default()}), ..default() }),
            MeshPickingPlugin,
            buildings::BuildingsPlugin,
            visual_effects::VisualEffectsPlugin,
            map_objects::MapObjectsPlugin,
            objectives::ObjectivesPlugin,
            research::ResearchUiPlugin,
            overlays::OverlaysPlugin,
            weaponry::WeaponryPlugin,
            ui::UiPlugin,
            units::UnitsPlugin,
            wisps::WispsPlugin,
        ))
        .add_plugins((
            lib_grid::grids::GridsPlugin,
            lib_core::LibCorePlugin,
            lib_inventory::LibInventoryPlugin,
            lib_ui::LibUiPlugin,
        ))
        .add_plugins(editor::EditorPlugin)
        // Pins the screen-space post-process pass order. Must come after every effect plugin
        // (ripple / quantum field / force field) so all their render-graph nodes already exist.
        .add_plugins(lib_core::post_processing::PostProcessOrderingPlugin)
        .add_systems(PostStartup, |mut commands: Commands| { commands.queue(LaunchAction::default()); })
        .run();
}

#[allow(dead_code)]
enum LaunchAction {
    ApplySQLMigrations,
    RebuildSQLMigrationsMetadata,
    StartMap(LoadMapConfig),
}
impl Default for LaunchAction {
    fn default() -> Self {
        LaunchAction::StartMap(LoadMapConfig {
            map_path: "maps/test_map.dwd".into(),
            game_start_state: GameState::Running,
            admin_mode: AdminMode::Disabled,
        })
    }
}
impl Command for LaunchAction {
    fn apply(self, world: &mut World) {
        match self {
            LaunchAction::ApplySQLMigrations => {
                let paths = Self::all_dwd_paths(world);
                Self::run_migrations_on_paths(&paths, false);
                world.resource_mut::<Messages<bevy::app::AppExit>>().write(bevy::app::AppExit::Success);
            }
            LaunchAction::RebuildSQLMigrationsMetadata => {
                let paths = Self::all_dwd_paths(world);
                Self::run_migrations_on_paths(&paths, true);
                world.resource_mut::<Messages<bevy::app::AppExit>>().write(bevy::app::AppExit::Success);
            }
            LaunchAction::StartMap(config) => {
                world.trigger(LoadGameSignal(config));
            }
        }
    }
}
impl LaunchAction {
    fn run_migrations_on_paths(paths: &[String], rebuild: bool) {
        for path in paths {
            if rebuild {
                Log::info().dev().tag(Tag::GameLoad).message(format!("Rebuilding migration metadata for '{path}'"));
            } else {
                Log::info().dev().tag(Tag::GameLoad).message(format!("Applying migrations to '{path}'"));
            }
            if let Err(e) = with_db_connection(path, |conn| {
                if rebuild {
                    conn.execute("DELETE FROM refinery_schema_history;", [])?;
                }
                db_migrations::migrations::runner().run(conn)?;
                Ok(())
            }) {
                let action = if rebuild { "Rebuild" } else { "Migration" };
                Log::error().dev().tag(Tag::GameLoad).message(format!("{action} failed for '{path}': {e}"));
            }
        }
        let done_msg = if rebuild { "Migration metadata rebuild complete" } else { "Migrations complete" };
        Log::info().dev().tag(Tag::GameLoad).message(done_msg);
    }
    fn all_dwd_paths(world: &World) -> Vec<String> {
        let mut paths = world.resource::<GameMapList>().paths();
        paths.push("test_save.dwd".to_string());
        paths
    }
}
