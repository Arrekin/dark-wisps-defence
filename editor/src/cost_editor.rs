use bevy_egui::egui;

use resources::prelude::{Cost, ResourceType};

/// Edits a `Vec<Cost>` in place — add, remove, and modify rows. Reusable
/// across any editor tab that exposes costs.
pub fn ui_cost_editor(ui: &mut egui::Ui, costs: &mut Vec<Cost>) {
    let mut to_remove = None;
    for (index, cost) in costs.iter_mut().enumerate() {
        ui.horizontal(|ui| {
            egui::ComboBox::from_id_salt(format!("cost_resource_{index}"))
                .selected_text(cost.resource_type.to_string())
                .show_ui(ui, |ui| {
                    for resource_type in ResourceType::iter() {
                        ui.selectable_value(
                            &mut cost.resource_type,
                            resource_type,
                            resource_type.to_string(),
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
        costs.push(Cost {
            resource_type: ResourceType::DarkOre,
            amount: 0,
        });
    }
}
