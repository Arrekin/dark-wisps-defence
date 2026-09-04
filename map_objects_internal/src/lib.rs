pub(crate) mod walls;
pub(crate) mod dark_ore;
pub(crate) mod dark_ore_canvas;
pub(crate) mod quantum_field;
pub(crate) mod quantum_field_face;
pub(crate) mod quantum_field_post_process;
pub(crate) mod wall_canvas;
pub(crate) mod wall_editor_ui;
pub(crate) mod wall_swatch;

use bevy::prelude::*;


pub struct MapObjectsPlugin;
impl Plugin for MapObjectsPlugin {
    fn build(&self, app: &mut App) {
        app
            .add_plugins((
                dark_ore::DarkOrePlugin,
                dark_ore_canvas::DarkOreCanvasPlugin,
                quantum_field::QuantumFieldPlugin,
                quantum_field_face::QuantumFieldFacePlugin,
                quantum_field_post_process::QuantumFieldPostProcessPlugin,
                wall_canvas::WallCanvasPlugin,
                wall_editor_ui::WallEditorUiPlugin,
                wall_swatch::WallSwatchPlugin,
                walls::WallPlugin,
            ));
    }
}
