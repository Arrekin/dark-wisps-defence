mod console;
mod cost_editor;
mod moment_picker;
mod objectives;
mod research;
mod summonings;

use bevy::prelude::*;
use bevy_egui::{EguiPrimaryContextPass, egui};
use strum::{AsRefStr, EnumIter, IntoEnumIterator};

use game_core::prelude::ShardType;
use shards::prelude::*;
use states::AdminMode;

pub struct EditorPlugin;
impl Plugin for EditorPlugin {
    fn build(&self, app: &mut App) {
        app
            .add_plugins(bevy_egui::EguiPlugin::default())
            .insert_resource(bevy_egui::EguiGlobalSettings {
                // Don't let bevy_egui grab the first camera it finds as the primary context.
                // With multiple cameras (main + off-screen preview cameras) the pick is
                // non-deterministic, which made egui randomly render into a preview viewport.
                // We attach `PrimaryEguiContext` to `MainCamera` ourselves instead.
                auto_create_primary_context: false,
                enable_absorb_bevy_input_system: true,
                ..default()
            })
            .add_plugins(console::LogConsolePlugin)
            .init_resource::<EditorState>()
            .add_systems(EguiPrimaryContextPass, editor_ui.run_if(in_state(AdminMode::Enabled)))
            ;
    }
}

#[derive(Resource, Default)]
pub struct EditorState {
    pub active_tab: EditorTab,
    pub selected_summoning: Option<Entity>,
    pub selected_objective: Option<Entity>,
    pub selected_research: Option<Entity>,
    pub scenario_filename: Option<String>,
    pub pending_overwrite_confirm: Option<String>,
}

#[derive(Default, Clone, Copy, PartialEq, Eq, EnumIter, AsRefStr)]
pub enum EditorTab {
    #[default]
    General,
    Summonings,
    Objectives,
    Research,
    Shards,
}

fn editor_ui(world: &mut World) {
    let ctx = {
        let mut query = world.query_filtered::<&mut bevy_egui::EguiContext, With<bevy_egui::PrimaryEguiContext>>();
        let Ok(mut egui_ctx) = query.single_mut(world) else { return };
        egui_ctx.get_mut().clone()
    };

    egui::Window::new("Editor")
        .resizable(true)
        .default_width(400.0)
        .show(&ctx, |ui| {
            let active_tab = world.resource::<EditorState>().active_tab;
            ui.horizontal(|ui| {
                for tab in EditorTab::iter() {
                    if ui.selectable_label(active_tab == tab, tab.as_ref()).clicked() {
                        world.resource_mut::<EditorState>().active_tab = tab;
                    }
                }
            });

            ui.separator();

            match active_tab {
                EditorTab::General => tab_general(ui, world),
                EditorTab::Summonings => summonings::tab_summonings(ui, world),
                EditorTab::Objectives => objectives::tab_objectives(ui, world),
                EditorTab::Research => research::tab_research(ui, world),
                EditorTab::Shards => tab_shards(ui, world),
            }
        });
}

fn tab_general(ui: &mut egui::Ui, world: &mut World) {
    ui.label("General settings");

    ui.separator();

    ui.heading("Save as Scenario");

    // Filename input
    let filename = world.resource::<EditorState>().scenario_filename.clone().unwrap_or_default();
    let mut filename = filename;
    let filename_changed = ui.horizontal(|ui| {
        ui.label("Filename:");
        ui.text_edit_singleline(&mut filename)
    }).inner.changed();
    world.resource_mut::<EditorState>().scenario_filename = if filename.is_empty() { None } else { Some(filename) };
    // Clear stale overwrite confirm if the filename changed
    if filename_changed {
        world.resource_mut::<EditorState>().pending_overwrite_confirm = None;
    }

    // Save-as-scenario button
    let filename = world.resource::<EditorState>().scenario_filename.clone();
    let can_save = filename.as_ref().map(|n| !n.is_empty()).unwrap_or(false);
    ui.add_enabled_ui(can_save, |ui| {
        if ui.button("Save as Scenario").clicked()
            && let Some(ref name) = filename
        {
            let path = format!("maps/{}.dwd", name);
            if std::path::Path::new(&path).exists() {
                world.resource_mut::<EditorState>().pending_overwrite_confirm = Some(name.clone());
            } else {
                world.commands().trigger(persistence::SaveGameSignal {
                    target: persistence::SaveTarget::Scenario(name.clone()),
                });
            }
        }
    });

    // Overwrite confirm dialog
    let pending = world.resource::<EditorState>().pending_overwrite_confirm.clone();
    if let Some(pending_name) = pending {
        ui.horizontal(|ui| {
            ui.colored_label(egui::Color32::RED, format!("'{}.dwd' exists. Overwrite?", pending_name));
            if ui.button("Yes, overwrite").clicked() {
                world.commands().trigger(persistence::SaveGameSignal {
                    target: persistence::SaveTarget::Scenario(pending_name),
                });
                world.resource_mut::<EditorState>().pending_overwrite_confirm = None;
            }
            if ui.button("Cancel").clicked() {
                world.resource_mut::<EditorState>().pending_overwrite_confirm = None;
            }
        });
    }
}

fn tab_shards(ui: &mut egui::Ui, world: &mut World) {
    ui.heading("Shard Inventory");

    ui.horizontal(|ui| {
        ui.menu_button("+ Add Shard", |ui| {
            for shard_type in ShardType::iter() {
                if ui.button(shard_type.to_string()).clicked() {
                    world.resource_mut::<ShardInventory>().add(shard_type, 1);
                    ui.close();
                }
            }
        });
    });

    ui.separator();

    let shards: Vec<(ShardType, usize)> = world.resource::<ShardInventory>().iter().collect();

    if shards.is_empty() {
        ui.label("No shards in inventory");
    } else {
        egui::ScrollArea::vertical().max_height(300.0).show(ui, |ui| {
            for (shard_type, count) in shards {
                ui.horizontal(|ui| {
                    ui.label(format!("{}: {}", shard_type, count));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button("−").clicked()
                            && world.resource::<ShardInventory>().has(shard_type) {
                                world.resource_mut::<ShardInventory>().remove(shard_type);
                            }
                        if ui.button("+").clicked() {
                            world.resource_mut::<ShardInventory>().add(shard_type, 1);
                        }
                    });
                });
            }
        });
    }
}
