mod buildings;
mod editor;
mod effects;
mod map_objects;
mod overlays;
mod objectives;
mod prelude;
mod projectiles;
mod ui;
mod units;
mod wisps;

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
            effects::EffectsPlugin,
            map_objects::MapObjectsPlugin,
            objectives::ObjectivesPlugin,
            overlays::OverlaysPlugin,
            projectiles::ProjectilesPlugin,
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
        .add_plugins((
            editor::EditorPlugin,
        ))
        // Warning: Bevy behaves weirdly when there are many Startup systems. If some plugins begin to not run at all, watch out for Startup systems at all places. Use Post/Pre Startup instead.
        .add_systems(PostStartup, |mut commands: Commands| commands.trigger(lib_core::persistence::load::LoadGameSignal("maps/test_map.dwd".to_string())))
        .run();
}