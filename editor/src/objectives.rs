use bevy::prelude::*;
use bevy_egui::egui;

use game_core::prelude::TriggerSource;
use narrative::prelude::{
    BuilderObjective, ObjectiveActivatedBy, ObjectiveDetails, ObjectiveEditorUi, ObjectiveGoalGroup,
    ObjectiveGoals, ObjectiveGoalRegistry,
};
use session::TriggerStartGame;

use super::EditorState;

pub fn tab_objectives(ui: &mut egui::Ui, world: &mut World) {
    ui.horizontal(|ui| {
        if ui.button("+ Objective").clicked() {
            let count = {
                let mut query = world.query::<&ObjectiveDetails>();
                query.iter(world).count() + 1
            };
            let start_game = {
                let mut query = world.query_filtered::<Entity, With<TriggerStartGame>>();
                query.single(world).ok()
            };
            let mut builder = BuilderObjective::new(format!("objective_{}", count));
            if let Some(trigger) = start_game {
                builder = builder.with_activated_by(trigger);
            }
            let objective = world.spawn(builder).id();
            world.resource_mut::<EditorState>().selected_objective = Some(objective);
        }
    });

    ui.separator();

    let objective_list: Vec<(Entity, String)> = {
        let mut query = world.query::<(Entity, &ObjectiveDetails)>();
        query
            .iter(world)
            .map(|(e, d)| (e, d.id_name.clone()))
            .collect()
    };

    egui::ScrollArea::vertical()
        .max_height(150.0)
        .show(ui, |ui| {
            for (entity, id_name) in &objective_list {
                let is_selected = world.resource::<EditorState>().selected_objective == Some(*entity);
                if ui.selectable_label(is_selected, id_name).clicked() {
                    world.resource_mut::<EditorState>().selected_objective = Some(*entity);
                }
            }
        });

    ui.separator();

    let selected = world.resource::<EditorState>().selected_objective;
    if let Some(selected) = selected {
        if world.get_entity(selected).is_ok() {
            ui_objective_editor(ui, world, selected);
        } else {
            world.resource_mut::<EditorState>().selected_objective = None;
            ui.label("No objective selected");
        }
    } else {
        ui.label("Select an objective to edit");
    }
}

fn ui_objective_editor(ui: &mut egui::Ui, world: &mut World, objective: Entity) {
    // Common fields: id_name
    ui.horizontal(|ui| {
        ui.label("ID:");
        if let Some(mut det) = world.entity_mut(objective).get_mut::<ObjectiveDetails>() {
            ui.text_edit_singleline(&mut det.id_name);
        }
    });

    // Activation entity picker
    ui_activation_picker(ui, world, objective);

    ui.separator();

    // Goals section
    ui_goals_section(ui, world, objective);

    ui.separator();

    // Delete button
    if ui.button("🗑 Delete Objective").clicked() {
        world.entity_mut(objective).despawn();
        world.resource_mut::<EditorState>().selected_objective = None;
    }
}

fn ui_activation_picker(ui: &mut egui::Ui, world: &mut World, objective: Entity) {
    // Snapshot trigger sources — two passes to avoid double-borrowing world.
    let raw: Vec<(Entity, Option<String>)> = {
        let mut query =
            world.query_filtered::<(Entity, Option<&ObjectiveDetails>), With<TriggerSource>>();
        query
            .iter(world)
            .map(|(e, d)| (e, d.map(|d| d.id_name.clone())))
            .collect()
    };
    let triggers: Vec<(Entity, String)> = raw
        .into_iter()
        .map(|(e, name)| {
            let name = name.unwrap_or_else(|| {
                if world.entity(e).contains::<TriggerStartGame>() {
                    "StartGame".to_string()
                } else {
                    format!("Trigger #{}", e.index())
                }
            });
            (e, name)
        })
        .collect();

    let current_trigger: Option<Entity> = world
        .entity(objective)
        .get::<ObjectiveActivatedBy>()
        .map(|a| a.0);

    ui.horizontal(|ui| {
        ui.label("Activated by:");
        let selected_text = current_trigger
            .and_then(|e| {
                triggers
                    .iter()
                    .find(|(te, _)| *te == e)
                    .map(|(_, n)| n.clone())
            })
            .unwrap_or_else(|| "—".to_string());
        egui::ComboBox::from_id_salt("activation_picker")
            .selected_text(selected_text)
            .show_ui(ui, |ui| {
                for (trigger_entity, name) in &triggers {
                    let is_selected = current_trigger == Some(*trigger_entity);
                    if ui.selectable_label(is_selected, name).clicked() {
                        // Insert directly — Bevy replaces the existing relationship
                        // without firing `On<Remove>`, which would trigger the
                        // lost-activation observer and fail Inactive objectives.
                        world
                            .entity_mut(objective)
                            .insert(ObjectiveActivatedBy(*trigger_entity));
                    }
                }
            });
    });
}

fn ui_goals_section(ui: &mut egui::Ui, world: &mut World, objective: Entity) {
    ui.heading("Goals");

    // Clone the registry to release the world borrow before the menu closure.
    let registry = world.resource::<ObjectiveGoalRegistry>().clone();
    use strum::IntoEnumIterator;
    let groups = ObjectiveGoalGroup::iter().collect::<Vec<_>>();

    ui.menu_button("+ Add Goal", |ui| {
        for group in &groups {
            ui.label(group.as_ref());
            for entry in registry.entries.iter().filter(|e| e.group == *group) {
                if ui.button(entry.name).clicked() {
                    (entry.spawn)(&mut world.commands(), objective);
                    ui.close();
                }
            }
            ui.separator();
        }
    });

    // Snapshot goal entities
    let goals: Vec<Entity> = world
        .entity(objective)
        .get::<ObjectiveGoals>()
        .map(|g| g.iter().collect())
        .unwrap_or_default();

    if goals.is_empty() {
        ui.label("⚠ No goals — objective will vacuously satisfy on activation");
    } else {
        for goal_entity in &goals {
            ui_goal_editor(ui, world, *goal_entity);
        }
    }
}

fn ui_goal_editor(ui: &mut egui::Ui, world: &mut World, goal_entity: Entity) {
    let goal_name = format!("Goal #{}", goal_entity.index());

    ui.collapsing(goal_name, |ui| {
        // Call the goal's ObjectiveEditorUi fn pointer
        let fn_ptr = world
            .entity(goal_entity)
            .get::<ObjectiveEditorUi>()
            .map(|e| e.0);
        if let Some(fn_ptr) = fn_ptr {
            fn_ptr(ui, &mut world.entity_mut(goal_entity));
        }

        // Remove button
        if ui.button("🗑 Remove Goal").clicked() {
            world.entity_mut(goal_entity).despawn();
        }
    });
}
