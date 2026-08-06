use bevy::prelude::*;
use bevy_egui::egui;
use strum::IntoEnumIterator;

use game_core::prelude::{DisplayName, MomentOf};
use narrative::prelude::*;
use session::MomentGameStart;

use super::EditorState;
use super::moment_picker::{ui_moment_picker, find_moment_child};

pub fn tab_objectives(ui: &mut egui::Ui, world: &mut World) {
    ui.horizontal(|ui| {
        if ui.button("+ Objective").clicked() {
            let count = {
                let mut query = world.query::<&ObjectiveDetails>();
                query.iter(world).count() + 1
            };
            let start_game = {
                let mut query = world.query_filtered::<Entity, With<MomentGameStart>>();
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
        let current = world.entity(objective).get::<ObjectiveDetails>().map(|d| d.id_name.clone());
        if let Some(mut id_name) = current {
            ui.text_edit_singleline(&mut id_name);
            if let Some(mut det) = world.entity_mut(objective).get_mut::<ObjectiveDetails>()
                && det.id_name != id_name
            {
                det.id_name = id_name;
            }
        }
    });

    // Activation entity picker
    ui_moment_picker(ui, world, objective, "objective_activation_picker");

    ui.separator();

    // Moments section
    ui_moments_section(ui, world, objective);

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

fn ui_moments_section(ui: &mut egui::Ui, world: &mut World, objective: Entity) {
    ui.heading("Moments");

    let mut satisfied = find_moment_child::<MomentObjectiveSatisfied>(world, objective).is_some();
    let mut failed = find_moment_child::<MomentObjectiveFailed>(world, objective).is_some();

    ui.horizontal(|ui| {
        if ui.checkbox(&mut satisfied, "Satisfied").changed() {
            if satisfied {
                world.spawn((MomentOf(objective), MomentObjectiveSatisfied));
            } else if let Some(child) = find_moment_child::<MomentObjectiveSatisfied>(world, objective) {
                world.entity_mut(child).despawn();
            }
        }
        if ui.checkbox(&mut failed, "Failed").changed() {
            if failed {
                world.spawn((MomentOf(objective), MomentObjectiveFailed));
            } else if let Some(child) = find_moment_child::<MomentObjectiveFailed>(world, objective) {
                world.entity_mut(child).despawn();
            }
        }
    });
}

fn ui_goals_section(ui: &mut egui::Ui, world: &mut World, objective: Entity) {
    ui.heading("Goals");

    let mut clicked = None;
    ui.menu_button("+ Add Goal", |ui| {
        let registry = world.resource::<ObjectiveGoalRegistry>();
        for group in ObjectiveGoalGroup::iter() {
            ui.label(group.as_ref());
            for (index, entry) in registry.entries.iter().enumerate().filter(|(_, e)| e.group == group) {
                if ui.button(entry.name).clicked() {
                    clicked = Some(index);
                    ui.close();
                }
            }
            ui.separator();
        }
    });
    if let Some(index) = clicked {
        let registry = world.resource::<ObjectiveGoalRegistry>();
        (registry.entries[index].spawn)(&mut world.commands(), objective);
    }

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
    let goal_name = world
        .entity(goal_entity)
        .get::<DisplayName>()
        .map(|name| name.0.clone())
        .unwrap_or_else(|| format!("Goal #{}", goal_entity.index()));

    egui::CollapsingHeader::new(goal_name)
        .id_salt(goal_entity)
        .show(ui, |ui| {
            let fn_ptr = world
                .entity(goal_entity)
                .get::<ObjectiveEditorUi>()
                .map(|e| e.0);
            if let Some(fn_ptr) = fn_ptr {
                fn_ptr(ui, &mut world.entity_mut(goal_entity));
            }

            if ui.button("🗑 Remove Goal").clicked() {
                world.entity_mut(goal_entity).despawn();
            }
        });
}
