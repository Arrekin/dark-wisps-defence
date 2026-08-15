use bevy::prelude::*;
use bevy_egui::egui;
use strum::IntoEnumIterator;

use hud::prelude::ShowGrid;
use map_objects::wall_style::WallCanvasDebug;

pub fn tab_debug(ui: &mut egui::Ui, world: &mut World) {
    ui.heading("Display");

    let mut show_grid = world.contains_resource::<ShowGrid>();
    if ui.checkbox(&mut show_grid, "Show grid").changed() {
        if show_grid {
            world.insert_resource(ShowGrid);
        } else {
            world.remove_resource::<ShowGrid>();
        }
    }

    ui.separator();

    ui.heading("Wall Canvas");

    // `apply_wall_canvas_debug` runs on `resource_changed`, so this only takes `resource_mut`
    // when the choice actually moved.
    let current = *world.resource::<WallCanvasDebug>();
    let mut selected = current;
    let selected_label = selected.as_ref().to_string();
    egui::ComboBox::from_label("Debug mode")
        .selected_text(selected_label)
        .show_ui(ui, |ui| {
            for mode in WallCanvasDebug::iter() {
                ui.selectable_value(&mut selected, mode, mode.as_ref());
            }
        });
    if selected != current {
        *world.resource_mut::<WallCanvasDebug>() = selected;
    }
}
