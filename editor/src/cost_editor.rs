use bevy_egui::egui;

use almanach::prelude::Almanach;
use resources::prelude::{ResourceAmount, ResourceType};

/// Edits a `Vec<ResourceAmount>` in place — add, remove, and modify rows. Reusable
/// across any editor tab that exposes costs.
pub fn ui_cost_editor(ui: &mut egui::Ui, almanach: &Almanach, costs: &mut Vec<ResourceAmount>) {
    let mut to_remove = None;
    for (index, cost) in costs.iter_mut().enumerate() {
        ui.horizontal(|ui| {
            egui::ComboBox::from_id_salt(format!("cost_resource_{index}"))
                .selected_text(&almanach.get_resource_info(cost.resource_type).name)
                .show_ui(ui, |ui| {
                    for resource_type in ResourceType::all() {
                        ui.selectable_value(
                            &mut cost.resource_type,
                            resource_type,
                            &almanach.get_resource_info(resource_type).name,
                        );
                    }
                });

            ui.add(
                egui::DragValue::new(&mut cost.amount)
                    .speed(1.0)
                    .range(0..=i32::MAX),
            );

            if ui.button("🗑").clicked() {
                to_remove = Some(index);
            }
        });
    }

    if let Some(index) = to_remove {
        costs.remove(index);
    }

    if ui.button("+ Add Cost").clicked() {
        costs.push(ResourceAmount::new(ResourceType::DarkOre, 0));
    }
}
