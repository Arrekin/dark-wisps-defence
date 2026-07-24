use bevy::prelude::*;
use bevy_egui::egui;
use strum::IntoEnumIterator;

use game_core::prelude::{Moment, MomentOf, MomentOfInterest, HasMoments};
use narrative::prelude::*;
use session::MomentGameStart;

use super::EditorState;

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
    ui_activation_picker(ui, world, objective);

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

fn ui_activation_picker(ui: &mut egui::Ui, world: &mut World, objective: Entity) {
    // Snapshot all moment entities with their display label. The label composes
    // from the parent's `id_name` (if the parent is an objective) + the
    // moment's own `Name`. Standalone moments (e.g. MomentGameStart) show just
    // their `Name`.
    let moments: Vec<(Entity, String)> = {
        let mut query = world.query_filtered::<(Entity, &Name, Option<&MomentOf>), With<Moment>>();
        query
            .iter(world)
            .map(|(e, name, parent_rel)| {
                let label = match parent_rel {
                    Some(rel) => {
                        let parent = world.entity(rel.0);
                        match parent.get::<ObjectiveDetails>() {
                            Some(det) => format!("{}: {}", det.id_name, name.as_str()),
                            None => name.as_str().to_string(),
                        }
                    }
                    None => name.as_str().to_string(),
                };
                (e, label)
            })
            .collect()
    };

    let current: Option<Entity> = world
        .entity(objective)
        .get::<MomentOfInterest>()
        .map(|a| a.0);

    ui.horizontal(|ui| {
        ui.label("Activated by:");
        let selected_text = current
            .and_then(|e| moments.iter().find(|(te, _)| *te == e).map(|(_, n)| n.clone()))
            .unwrap_or_else(|| "—".to_string());
        egui::ComboBox::from_id_salt("activation_picker")
            .selected_text(selected_text)
            .show_ui(ui, |ui| {
                for (moment_entity, label) in &moments {
                    let is_selected = current == Some(*moment_entity);
                    if ui.selectable_label(is_selected, label).clicked() {
                        // Insert directly — Bevy replaces the existing relationship
                        // without firing `On<Remove>`, which would trigger the
                        // lost-activation observer and fail Inactive objectives.
                        world
                            .entity_mut(objective)
                            .insert(MomentOfInterest(*moment_entity));
                    }
                }
            });
    });
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

/// Find the moment child of `objective` that has marker `T`.
fn find_moment_child<T: Component>(world: &World, objective: Entity) -> Option<Entity> {
    world
        .entity(objective)
        .get::<HasMoments>()
        .into_iter()
        .flat_map(|h| h.iter())
        .find(|&c| world.entity(c).contains::<T>())
}

fn ui_goals_section(ui: &mut egui::Ui, world: &mut World, objective: Entity) {
    ui.heading("Goals");

    // Clone the registry to release the world borrow before the menu closure.
    let registry = world.resource::<ObjectiveGoalRegistry>().clone();

    ui.menu_button("+ Add Goal", |ui| {
        for group in ObjectiveGoalGroup::iter() {
            ui.label(group.as_ref());
            for entry in registry.entries.iter().filter(|e| e.group == group) {
                if ui.button(entry.name).clicked() {
                    (entry.spawn)(&mut world.commands(), objective);
                    ui.close();
                }
            }
            ui.separator();
        }
    });

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
