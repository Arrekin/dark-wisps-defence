pub(crate) mod walls;
pub(crate) mod dark_ore;
pub(crate) mod quantum_field;
pub(crate) mod quantum_field_post_process;
pub(crate) mod wall_canvas;
pub(crate) mod wall_editor_ui;

use bevy::prelude::*;


pub struct MapObjectsPlugin;
impl Plugin for MapObjectsPlugin {
    fn build(&self, app: &mut App) {
        app
            .add_plugins((
                dark_ore::DarkOrePlugin,
                quantum_field::QuantumFieldPlugin,
                quantum_field_post_process::QuantumFieldPostProcessPlugin,
                wall_canvas::WallCanvasPlugin,
                wall_editor_ui::WallEditorUiPlugin,
                walls::WallPlugin,
            ));
    }
}
