pub(crate) mod common;
pub(crate) mod dark_ore;
pub(crate) mod dark_ore_canvas;
pub(crate) mod dark_ore_materials;
pub(crate) mod excavation_editor_ui;
pub(crate) mod excavation_materials;
pub(crate) mod excavations;
pub(crate) mod quantum_field;
pub(crate) mod quantum_field_anomaly;
pub(crate) mod quantum_field_materials;
pub(crate) mod tunnel_reveal;
pub(crate) mod underground_canvas;
pub(crate) mod volume_shape_controls;
pub(crate) mod wall_canvas;
pub(crate) mod wall_editor_ui;
pub(crate) mod wall_materials;
pub(crate) mod walls;

use bevy::prelude::*;

pub struct MapObjectsPlugin;
impl Plugin for MapObjectsPlugin {
    fn build(&self, app: &mut App) {
        app
            .add_plugins((
                dark_ore::DarkOrePlugin,
                dark_ore_canvas::DarkOreCanvasPlugin,
                dark_ore_materials::DarkOreMaterialsPlugin,
                excavation_editor_ui::ExcavationEditorUiPlugin,
                excavation_materials::ExcavationMaterialsPlugin,
                excavations::ExcavationPlugin,
                quantum_field::QuantumFieldPlugin,
                quantum_field_materials::QuantumFieldMaterialsPlugin,
                quantum_field_anomaly::QuantumFieldAnomalyPlugin,
                tunnel_reveal::TunnelRevealPlugin,
                underground_canvas::UndergroundCanvasPlugin,
                wall_canvas::WallCanvasPlugin,
                wall_editor_ui::WallEditorUiPlugin,
                wall_materials::WallMaterialsPlugin,
                walls::WallPlugin,
            ));
    }
}
