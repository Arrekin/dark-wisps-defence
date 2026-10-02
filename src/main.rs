use bevy::{prelude::*, window::PresentMode};

use persistence::{LoadGameSignal, LoadMapConfig, MapFileName, MapSource, list_map_file_names, run_migrations_on_paths};
use states::{AdminMode, prelude::*};

fn main() {
    App::new()
        .insert_resource(ClearColor(Color::srgb_u8(30, 31, 34)))
        .add_plugins((
            DefaultPlugins
                .set(ImagePlugin::default_nearest())
                // Warning: VSync is causing a lot of issues with mouse events processing
                .set(WindowPlugin { primary_window: Some(Window { present_mode: PresentMode::AutoNoVsync, ..default() }), ..default() }),
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
        .add_plugins(byoaic::ByoaicPlugin)
        .add_systems(PostStartup, |mut commands: Commands| { commands.queue(LaunchAction::default()); })
        .run();
}

#[expect(dead_code, reason = "alternative launch actions, selected by editing `LaunchAction::default`")]
enum LaunchAction {
    ApplySQLMigrations,
    RebuildSQLMigrationsMetadata,
    StartMap(LoadMapConfig),
}
impl Default for LaunchAction {
    /// The launch switchboard: edit these fields by hand to start another map, paused, or in admin
    /// mode. Every field stays spelled out on purpose; do not replace this with a
    /// `LoadMapConfig` constructor, even when the values happen to match one.
    fn default() -> Self {
        LaunchAction::StartMap(LoadMapConfig {
            source: MapSource::File("maps/test_map.dwd".into()),
            game_start_state: GameState::Running,
            admin_mode: AdminMode::Disabled,
            response: default(),
        })
    }
}
impl Command for LaunchAction {
    type Out = ();
    fn apply(self, world: &mut World) {
        match self {
            LaunchAction::ApplySQLMigrations => {
                run_migrations_on_paths(&Self::all_dwd_paths(), false);
                world.write_message(AppExit::Success);
            }
            LaunchAction::RebuildSQLMigrationsMetadata => {
                run_migrations_on_paths(&Self::all_dwd_paths(), true);
                world.write_message(AppExit::Success);
            }
            LaunchAction::StartMap(config) => {
                world.trigger(LoadGameSignal(config));
            }
        }
    }
}
impl LaunchAction {
    fn all_dwd_paths() -> Vec<String> {
        let mut paths: Vec<String> = list_map_file_names().iter().map(MapFileName::path).collect();
        paths.push("test_save.dwd".to_string());
        paths
    }
}
