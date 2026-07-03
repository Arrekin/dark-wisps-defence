use bevy::{ecs::message::Messages, prelude::*};

use persistence::{GameMapList, LoadGameSignal, LoadMapConfig, run_migrations_on_paths};
use states::{AdminMode, prelude::*};

fn main() {
    App::new()
        .insert_resource(ClearColor(Color::srgb_u8(30, 31, 34)))
        .add_plugins((
            DefaultPlugins
                .set(ImagePlugin::default_nearest())
                // Warning: VSync is causing a lot of issues with mouse events processing
                .set(WindowPlugin{ primary_window: Some(Window { present_mode: bevy::window::PresentMode::AutoNoVsync, ..default()}), ..default() }),
            MeshPickingPlugin,
            buildings_internal::BuildingsPlugin,
            map_objects_internal::MapObjectsPlugin,
            narrative_internal::NarrativePlugin,
            overlays::OverlaysPlugin,
            weaponry_internal::WeaponryPlugin,
            hud_internal::HudPlugin,
            units_internal::UnitsPlugin,
            wisps_internal::WispsPlugin,
        ))
        .add_plugins((
            game_core_internal::GameCorePlugin,
            logging::LoggingPlugin,
            states::StatesPlugin,
            persistence::PersistencePlugin,
            viewport::ViewportPlugin,
            session::SessionPlugin,
            alteration_internal::AlterationPlugin,
            resources_internal::ResourcesPlugin,
            shards_internal::ShardsPlugin,
            research_internal::ResearchPlugin,
        ))
        .add_plugins((
            grids_internal::GridsPlugin,
            almanach::AlmanachPlugin,
            widgets_internal::WidgetsPlugin,
        ))
        .add_plugins(editor::EditorPlugin)
        .add_plugins(visuals_internal::VisualsPlugin)
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
    type Out = ();
    fn apply(self, world: &mut World) {
        match self {
            LaunchAction::ApplySQLMigrations => {
                let paths = Self::all_dwd_paths(world);
                run_migrations_on_paths(&paths, false);
                world.resource_mut::<Messages<bevy::app::AppExit>>().write(bevy::app::AppExit::Success);
            }
            LaunchAction::RebuildSQLMigrationsMetadata => {
                let paths = Self::all_dwd_paths(world);
                run_migrations_on_paths(&paths, true);
                world.resource_mut::<Messages<bevy::app::AppExit>>().write(bevy::app::AppExit::Success);
            }
            LaunchAction::StartMap(config) => {
                world.trigger(LoadGameSignal(config));
            }
        }
    }
}
impl LaunchAction {
    fn all_dwd_paths(world: &World) -> Vec<String> {
        let mut paths = world.resource::<GameMapList>().paths();
        paths.push("test_save.dwd".to_string());
        paths
    }
}
