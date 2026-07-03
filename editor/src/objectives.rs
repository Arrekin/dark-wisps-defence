use bevy::prelude::*;
use bevy_egui::egui;

use narrative::prelude::*;

use super::EditorState;

pub fn tab_objectives(
    ui: &mut egui::Ui,
    state: &mut ResMut<EditorState>,
    commands: &mut Commands,
    objectives: &mut Query<(Entity, &mut ObjectiveDetails, Option<&mut ObjectiveKillWisps>)>,
) {
    ui.horizontal(|ui| {
        ui.menu_button("+ Add Objective", |ui| {
            let count = objectives.iter().count() + 1;
            if ui.button("Clear All Quantum Fields").clicked() {
                let obj = ObjectiveDetails::new(
                    format!("objective_{}", count),
                    ObjectiveType::ClearAllQuantumFields,
                    "game-started".to_string(),
                );
                let entity = commands.spawn(BuilderObjective::new(obj)).id();
                state.selected_objective = Some(entity);
                ui.close();
            }
            if ui.button("Kill Wisps").clicked() {
                let obj = ObjectiveDetails::new(
                    format!("objective_{}", count),
                    ObjectiveType::KillWisps(10),
                    "game-started".to_string(),
                );
                let entity = commands.spawn(BuilderObjective::new(obj)).id();
                state.selected_objective = Some(entity);
                ui.close();
            }
        });
    });
    
    ui.separator();
    
    let objective_list: Vec<_> = objectives.iter().map(|(e, o, _)| (e, o.id_name.clone())).collect();
    
    egui::ScrollArea::vertical().max_height(150.0).show(ui, |ui| {
        for (entity, id_name) in &objective_list {
            let is_selected = state.selected_objective == Some(*entity);
            if ui.selectable_label(is_selected, id_name).clicked() {
                state.selected_objective = Some(*entity);
            }
        }
    });
    
    ui.separator();
    
    if let Some(selected) = state.selected_objective {
        if let Ok((entity, mut objective, kill_wisps)) = objectives.get_mut(selected) {
            ui_objective_editor(ui, entity, &mut objective, kill_wisps, commands, state);
        } else {
            ui.label("No objective selected");
        }
    } else {
        ui.label("Select an objective to edit");
    }
}

fn ui_objective_editor(
    ui: &mut egui::Ui,
    entity: Entity,
    objective: &mut Mut<ObjectiveDetails>,
    kill_wisps: Option<Mut<ObjectiveKillWisps>>,
    commands: &mut Commands,
    state: &mut ResMut<EditorState>,
) {
    ui.horizontal(|ui| {
        ui.label("ID:");
        ui.text_edit_singleline(&mut objective.id_name);
    });
    
    ui.horizontal(|ui| {
        ui.label("Activation Event:");
        ui.text_edit_singleline(&mut objective.activation_event);
    });
    
    ui.horizontal(|ui| {
        ui.label("Type:");
        ui.label(match objective.objective_type {
            ObjectiveType::ClearAllQuantumFields => "Clear All Quantum Fields",
            ObjectiveType::KillWisps(_) => "Kill Wisps",
        });
    });
    
    match &objective.objective_type {
        ObjectiveType::ClearAllQuantumFields => {
            ui.label("Clear all quantum fields on the map");
        }
        ObjectiveType::KillWisps(_) => {
            if let Some(mut kw) = kill_wisps {
                ui.horizontal(|ui| {
                    ui.label("Target Count:");
                    let mut count = kw.target_amount as i32;
                    if ui.add(egui::DragValue::new(&mut count).range(1..=10000)).changed() {
                        kw.target_amount = count as usize;
                    }
                });
            }
        }
    }
    
    ui.separator();
    
    if ui.button("🗑 Delete Objective").clicked() {
        commands.entity(entity).despawn();
        state.selected_objective = None;
    }
}
