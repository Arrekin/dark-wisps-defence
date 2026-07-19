use bevy::prelude::*;
use bevy_egui::egui;
use strum::IntoEnumIterator;

use game_core::prelude::*;
use wisps::summoning::{BuilderSummoning, EdgeSide, SpawnArea, SpawnTempo, Summoning};

use super::EditorState;

pub fn tab_summonings(
    ui: &mut egui::Ui,
    state: &mut ResMut<EditorState>,
    commands: &mut Commands,
    summonings: &mut Query<(Entity, &mut Summoning)>,
) {
    ui.horizontal(|ui| {
        if ui.button("+ Add Summoning").clicked() {
            let new_summoning = Summoning { id_name: format!("summoning_{}", summonings.iter().count() + 1), ..Default::default() };
            let entity: Entity = commands.spawn(BuilderSummoning::new(new_summoning)).id();
            state.selected_summoning = Some(entity);
        }
    });
    
    ui.separator();
    
    let summoning_list: Vec<_> = summonings.iter().map(|(e, s)| (e, s.id_name.clone())).collect();
    
    egui::ScrollArea::vertical().max_height(150.0).show(ui, |ui| {
        for (entity, id_name) in &summoning_list {
            let is_selected = state.selected_summoning == Some(*entity);
            if ui.selectable_label(is_selected, id_name).clicked() {
                state.selected_summoning = Some(*entity);
            }
        }
    });
    
    ui.separator();
    
    if let Some(selected) = state.selected_summoning {
        if let Ok((entity, mut summoning)) = summonings.get_mut(selected) {
            ui_summoning_editor(ui, entity, &mut summoning, commands, state);
        } else {
            ui.label("No summoning selected");
        }
    } else {
        ui.label("Select a summoning to edit");
    }
}

fn ui_summoning_editor(
    ui: &mut egui::Ui,
    entity: Entity,
    summoning: &mut Mut<Summoning>,
    commands: &mut Commands,
    state: &mut ResMut<EditorState>,
) {
    ui.horizontal(|ui| {
        ui.label("ID:");
        ui.text_edit_singleline(&mut summoning.id_name);
    });
    
    ui.horizontal(|ui| {
        ui.label("Activation Event:");
        ui.text_edit_singleline(&mut summoning.activation_event);
    });
    
    ui.collapsing("Wisp Types", |ui| {
        for wisp_type in WispType::iter() {
            let mut enabled = summoning.wisp_types.contains(&wisp_type);
            if ui.checkbox(&mut enabled, wisp_type.as_ref()).changed() {
                if enabled {
                    if !summoning.wisp_types.contains(&wisp_type) {
                        summoning.wisp_types.push(wisp_type);
                    }
                } else if summoning.wisp_types.len() > 1 {
                    summoning.wisp_types.retain(|t| *t != wisp_type);
                }
            }
        }
    });
    
    ui.collapsing("Spawn Area", |ui| {
        ui_spawn_area(ui, &mut summoning.area);
    });
    
    ui.collapsing("Spawn Tempo", |ui| {
        ui_spawn_tempo(ui, &mut summoning.tempo);
    });
    
    ui.horizontal(|ui| {
        let mut has_limit = summoning.limit_count.is_some();
        if ui.checkbox(&mut has_limit, "Limit Count").changed() {
            summoning.limit_count = if has_limit { Some(100) } else { None };
        }
        if let Some(ref mut limit) = summoning.limit_count {
            ui.add(egui::DragValue::new(limit).range(1..=10000));
        }
    });
    
    ui.separator();
    
    if ui.button("🗑 Delete Summoning").clicked() {
        commands.entity(entity).despawn();
        state.selected_summoning = None;
    }
}

fn ui_spawn_area(ui: &mut egui::Ui, area: &mut SpawnArea) {
    egui::ComboBox::from_label("Type")
        .selected_text(area.as_ref())
        .show_ui(ui, |ui| {
            if ui.selectable_label(matches!(area, SpawnArea::Coords { .. }), SpawnArea::Coords { coords: vec![] }.as_ref()).clicked() {
                *area = SpawnArea::Coords { coords: vec![GridCoords { x: 0, y: 0 }] };
            }
            if ui.selectable_label(matches!(area, SpawnArea::Rect { .. }), SpawnArea::Rect { origin: GridCoords::default(), width: 0, height: 0 }.as_ref()).clicked() {
                *area = SpawnArea::Rect { origin: GridCoords { x: 0, y: 0 }, width: 10, height: 10 };
            }
            if ui.selectable_label(matches!(area, SpawnArea::Edge { .. }), SpawnArea::Edge { side: EdgeSide::default() }.as_ref()).clicked() {
                *area = SpawnArea::Edge { side: EdgeSide::default() };
            }
            if ui.selectable_label(matches!(area, SpawnArea::EdgesAll), SpawnArea::EdgesAll.as_ref()).clicked() {
                *area = SpawnArea::EdgesAll;
            }
        });
    
    match area {
        SpawnArea::Coords { coords } => {
            ui.horizontal(|ui| {
                ui.label(format!("{} coords", coords.len()));
                if ui.button("+").clicked() {
                    coords.push(GridCoords { x: 0, y: 0 });
                }
            });
            
            let mut to_remove = None;
            let can_remove = coords.len() > 1;
            egui::ScrollArea::vertical().max_height(100.0).show(ui, |ui| {
                for (i, coord) in coords.iter_mut().enumerate() {
                    ui.horizontal(|ui| {
                        ui.label(format!("{}:", i));
                        ui.add(egui::DragValue::new(&mut coord.x).prefix("x:"));
                        ui.add(egui::DragValue::new(&mut coord.y).prefix("y:"));
                        if can_remove && ui.button("🗑").clicked() {
                            to_remove = Some(i);
                        }
                    });
                }
            });
            
            if let Some(i) = to_remove {
                coords.remove(i);
            }
        }
        SpawnArea::Rect { origin, width, height } => {
            ui.horizontal(|ui| {
                ui.label("Origin X:");
                ui.add(egui::DragValue::new(&mut origin.x));
                ui.label("Y:");
                ui.add(egui::DragValue::new(&mut origin.y));
            });
            ui.horizontal(|ui| {
                ui.label("Width:");
                ui.add(egui::DragValue::new(width).range(1..=1000));
                ui.label("Height:");
                ui.add(egui::DragValue::new(height).range(1..=1000));
            });
        }
        SpawnArea::Edge { side } => {
            egui::ComboBox::from_label("Side")
                .selected_text(side.as_ref())
                .show_ui(ui, |ui| {
                    for s in EdgeSide::iter() {
                        if ui.selectable_label(*side == s, s.as_ref()).clicked() {
                            *side = s;
                        }
                    }
                });
        }
        SpawnArea::EdgesAll => {
            ui.label("Spawns from all edges");
        }
    }
}

fn ui_spawn_tempo(ui: &mut egui::Ui, tempo: &mut SpawnTempo) {
    match tempo {
        SpawnTempo::Continuous { seconds, jitter, bulk_count } => {
            ui.horizontal(|ui| {
                ui.label("Interval (s):");
                ui.add(egui::DragValue::new(seconds).speed(0.01).range(0.01..=60.0));
            });
            ui.horizontal(|ui| {
                ui.label("Jitter:");
                ui.add(egui::DragValue::new(jitter).speed(0.01).range(0.0..=10.0));
            });
            ui.horizontal(|ui| {
                ui.label("Bulk Count:");
                ui.add(egui::DragValue::new(bulk_count).range(1..=100));
            });
        }
    }
}
