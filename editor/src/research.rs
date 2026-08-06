use std::collections::HashSet;
use std::time::Duration;

use bevy::prelude::*;
use bevy_egui::egui;

use almanach::prelude::{Almanach, ResearchSpawnFn};
use game_core::prelude::{ContentId, DisplayDescription, DisplayIconSwitcher, DisplayName};
use outcomes::prelude::*;
use research::prelude::{Research, ResearchState};

use super::EditorState;

pub fn tab_research(ui: &mut egui::Ui, world: &mut World) {
    ui.horizontal(|ui| {
        if ui.button("Seed Missing").clicked() {
            seed_missing(world);
        }
    });

    ui.separator();

    struct ResearchRow {
        entity: Entity,
        name: String,
        enabled: bool,
    }

    let mut researches: Vec<ResearchRow> = {
        let mut query = world.query_filtered::<(Entity, &DisplayName, Option<&ResearchState>), With<Research>>();
        query
            .iter(world)
            .map(|(entity, name, state)| ResearchRow {
                entity,
                name: name.0.clone(),
                enabled: state.is_some(),
            })
            .collect()
    };
    researches.sort_by(|a, b| a.name.cmp(&b.name));

    let mut selected = world.resource::<EditorState>().selected_research;

    egui::ScrollArea::vertical()
        .max_height(150.0)
        .show(ui, |ui| {
            for row in &researches {
                ui.horizontal(|ui| {
                    let is_selected = selected == Some(row.entity);
                    if row.enabled {
                        if ui.selectable_label(is_selected, &row.name).clicked() {
                            selected = Some(row.entity);
                        }
                    } else {
                        ui.add_enabled(false, egui::Label::new(format!("{} (not in scenario)", row.name)));
                    }

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if row.enabled {
                            if ui.button("Remove from scenario").clicked() {
                                remove_from_scenario(world, row.entity);
                                if selected == Some(row.entity) {
                                    selected = None;
                                }
                            }
                        } else {
                            if ui.button("Add to scenario").clicked() {
                                world.entity_mut(row.entity).insert(ResearchState::Available);
                            }
                        }
                    });
                });
            }
        });

    ui.separator();

    if let Some(selected_entity) = selected {
        if world.get_entity(selected_entity).is_ok() {
            ui_research_editor(ui, world, selected_entity);
        } else {
            selected = None;
            ui.label("No research selected");
        }
    } else {
        ui.label("Select a research to edit");
    }

    world.resource_mut::<EditorState>().selected_research = selected;
}

fn ui_research_editor(ui: &mut egui::Ui, world: &mut World, research: Entity) {
    ui.horizontal(|ui| {
        ui.label("Content ID:");
        let Some(id) = world.entity(research).get::<ContentId>() else { return };
        ui.label(&id.0);
    });

    ui.horizontal(|ui| {
        ui.label("Name:");
        let mut entity_ref = world.entity_mut(research);
        let Some(mut name) = entity_ref.get_mut::<DisplayName>() else { return };
        ui.text_edit_singleline(&mut name.0);
    });

    ui.horizontal(|ui| {
        ui.label("Description:");
        let mut entity_ref = world.entity_mut(research);
        let Some(mut desc) = entity_ref.get_mut::<DisplayDescription>() else { return };
        ui.text_edit_multiline(&mut desc.0);
    });

    ui.horizontal(|ui| {
        ui.label("Icon:");
        let mut entity_ref = world.entity_mut(research);
        let Some(mut icon) = entity_ref.get_mut::<DisplayIconSwitcher>() else { return };
        ui.text_edit_singleline(&mut icon.0);
    });

    ui.horizontal(|ui| {
        ui.label("Duration (s):");
        let mut entity_ref = world.entity_mut(research);
        let Some(mut research_data) = entity_ref.get_mut::<Research>() else { return };
        let mut secs = research_data.duration.as_secs_f32();
        ui.add(egui::DragValue::new(&mut secs).speed(0.1).range(0.0..=f32::MAX));
        research_data.duration = Duration::from_secs_f32(secs);
    });

    ui.heading("Costs");
    let mut entity_ref = world.entity_mut(research);
    let Some(mut research_data) = entity_ref.get_mut::<Research>() else { return };
    super::cost_editor::ui_cost_editor(ui, &mut research_data.cost);

    ui.separator();
    ui_outcomes_section(ui, world, research);
}

fn ui_outcomes_section(ui: &mut egui::Ui, world: &mut World, research: Entity) {
    ui.heading("Outcomes");

    let outcomes: Vec<Entity> = world
        .entity(research)
        .get::<HasOutcomes>()
        .map(|has_outcomes| has_outcomes.iter().collect())
        .unwrap_or_default();

    for outcome_entity in outcomes {
        let title = world
            .entity(outcome_entity)
            .get::<DisplayName>()
            .map(|name| name.0.clone())
            .unwrap_or_else(|| format!("Outcome #{}", outcome_entity.index()));

        egui::CollapsingHeader::new(title)
            .id_salt(outcome_entity)
            .show(ui, |ui| {
                let editor_fn = world
                    .entity(outcome_entity)
                    .get::<OutcomeEditorUi>()
                    .map(|editor_ui| editor_ui.0);
                if let Some(editor_fn) = editor_fn {
                    editor_fn(ui, &mut world.entity_mut(outcome_entity));
                }

                if ui.button("🗑 Remove").clicked() {
                    world.entity_mut(outcome_entity).despawn();
                }
            });
    }

    let mut clicked = None;
    ui.menu_button("+ Add Outcome", |ui| {
        let registry = world.resource::<OutcomeKindRegistry>();
        for (index, entry) in registry.entries.iter().enumerate() {
            if ui.button(entry.name).clicked() {
                clicked = Some(index);
                ui.close();
            }
        }
    });
    if let Some(index) = clicked {
        let registry = world.resource::<OutcomeKindRegistry>();
        (registry.entries[index].spawn)(&mut world.commands(), research);
    }
}

/// Spawns disabled defaults for every catalog research the map lacks.
fn seed_missing(world: &mut World) {
    let existing: HashSet<ContentId> = {
        let mut query = world.query_filtered::<&ContentId, With<Research>>();
        query.iter(world).cloned().collect()
    };
    let to_spawn: Vec<(ContentId, ResearchSpawnFn)> = {
        let almanach = world.resource::<Almanach>();
        almanach
            .researches
            .iter()
            .filter(|(id, _)| !existing.contains(*id))
            .map(|(id, fn_ptr)| (id.clone(), *fn_ptr))
            .collect()
    };
    for (id, spawn_fn) in to_spawn {
        spawn_fn(&mut world.commands(), &id);
    }
}

/// Removes a research from the scenario: despawn the current entity and
/// respawn a fresh out-of-scenario one from the Almanach. Custom researches
/// (no catalog entry) are simply despawned.
fn remove_from_scenario(world: &mut World, research: Entity) {
    let Some(content_id) = world.entity(research).get::<ContentId>().cloned() else {
        world.entity_mut(research).despawn();
        return;
    };
    let spawn_fn = world.resource::<Almanach>()
        .researches
        .get(&content_id)
        .copied();
    world.entity_mut(research).despawn();
    if let Some(spawn_fn) = spawn_fn {
        spawn_fn(&mut world.commands(), &content_id);
    }
}
