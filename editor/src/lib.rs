mod console;
mod cost_editor;
mod debug;
mod moment_picker;
mod objectives;
mod research;
mod summonings;

use bevy::prelude::*;
use bevy_egui::{EguiPrimaryContextPass, egui};
use strum::{AsRefStr, EnumIter, IntoEnumIterator};

use game_core::prelude::{MapInfo, Shard};
use persistence::{InvalidFileName, LoadGameSignal, LoadMapConfig, MapFile, MapFileName, SaveGameSignal};
use resources::prelude::Stock;
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
            .add_systems(EguiPrimaryContextPass, editor_ui.run_if(in_state(AdminMode::Enabled)));
    }
}

#[derive(Resource, Default)]
pub struct EditorState {
    pub active_tab: EditorTab,
    pub selected_summoning: Option<Entity>,
    pub selected_objective: Option<Entity>,
    pub selected_research: Option<Entity>,
    pub scenario_file_name: String,
    pub pending_overwrite_confirm: Option<MapFile>,
    pub new_map_form: NewMapForm,
}
impl EditorState {
    /// Reset editor state for a freshly loaded map.
    fn reset_for_new_map(&mut self, name: &str) {
        self.selected_summoning = None;
        self.selected_objective = None;
        self.selected_research = None;
        self.scenario_file_name = name.to_string();
        self.pending_overwrite_confirm = None;
        self.new_map_form.pending_confirm = false;
    }
}

pub struct NewMapForm {
    pub name: String,
    pub width: i32,
    pub height: i32,
    pub pending_confirm: bool,
}
impl Default for NewMapForm {
    fn default() -> Self {
        Self { name: String::new(), width: MAP_WIDTH_DEFAULT, height: MAP_HEIGHT_DEFAULT, pending_confirm: false }
    }
}

const MAP_SIZE_MIN: i32 = 20;
const MAP_SIZE_MAX: i32 = 300;
const MAP_WIDTH_DEFAULT: i32 = 150;
const MAP_HEIGHT_DEFAULT: i32 = 100;

#[derive(Default, Clone, Copy, PartialEq, Eq, EnumIter, AsRefStr)]
pub enum EditorTab {
    #[default]
    General,
    Summonings,
    Objectives,
    Research,
    Shards,
    Debug,
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
                EditorTab::Debug => debug::tab_debug(ui, world),
            }
        });
}

fn create_new_map(world: &mut World, name: &str, width: i32, height: i32) {
    let map_info = MapInfo::new(name, (width, height));
    world.commands().trigger(LoadGameSignal(LoadMapConfig::new_map(map_info)));
    world.resource_mut::<EditorState>().reset_for_new_map(name);
}

fn tab_general(ui: &mut egui::Ui, world: &mut World) {
    // Read-only map info header
    let info = world.resource::<MapInfo>();
    ui.label(format!("Map: {}  ({} × {})", info.name, info.grid_bounds.width, info.grid_bounds.height));

    ui.separator();

    ui.heading("Save as Scenario");

    // File name input
    let file_name = {
        let mut editor = world.resource_mut::<EditorState>();
        let file_name_changed = ui.horizontal(|ui| {
            ui.label("Filename:");
            ui.text_edit_singleline(&mut editor.scenario_file_name)
        }).inner.changed();
        if file_name_changed {
            editor.pending_overwrite_confirm = None;
        }
        MapFileName::try_new(editor.scenario_file_name.as_str())
    };
    if let Err(invalid) = &file_name && *invalid != InvalidFileName::Empty {
        ui.colored_label(egui::Color32::RED, format!("Invalid filename: {invalid}"));
    }

    // Save-as-scenario button
    ui.add_enabled_ui(file_name.is_ok(), |ui| {
        if ui.button("Save as Scenario").clicked()
            && let Ok(file_name) = file_name
        {
            let file = MapFile::Scenario(file_name);
            if file.exists() {
                world.resource_mut::<EditorState>().pending_overwrite_confirm = Some(file);
            } else {
                world.commands().trigger(SaveGameSignal::new(file));
            }
        }
    });

    // Overwrite confirm dialog
    let pending = world.resource::<EditorState>().pending_overwrite_confirm.clone();
    if let Some(pending_file) = pending {
        ui.horizontal(|ui| {
            ui.colored_label(egui::Color32::RED, format!("'{}' exists. Overwrite?", pending_file.path()));
            if ui.button("Yes, overwrite").clicked() {
                world.commands().trigger(SaveGameSignal::new(pending_file));
                world.resource_mut::<EditorState>().pending_overwrite_confirm = None;
            }
            if ui.button("Cancel").clicked() {
                world.resource_mut::<EditorState>().pending_overwrite_confirm = None;
            }
        });
    }

    ui.separator();

    ui.heading("New Map");

    let pending_confirm = world.resource::<EditorState>().new_map_form.pending_confirm;
    let (name, width, height) = {
        let mut editor = world.resource_mut::<EditorState>();
        let form = &mut editor.new_map_form;
        ui.horizontal(|ui| {
            ui.label("Name:");
            ui.add_enabled_ui(!pending_confirm, |ui| {
                ui.text_edit_singleline(&mut form.name);
            });
        });
        ui.horizontal(|ui| {
            ui.label("Size:");
            ui.add_enabled_ui(!pending_confirm, |ui| {
                ui.add(egui::DragValue::new(&mut form.width).range(MAP_SIZE_MIN..=MAP_SIZE_MAX).prefix("W "));
                ui.add(egui::DragValue::new(&mut form.height).range(MAP_SIZE_MIN..=MAP_SIZE_MAX).prefix("H "));
            });
        });
        (form.name.clone(), form.width, form.height)
    };
    let name_valid = !name.is_empty() && name.chars().all(|character| character.is_ascii_alphanumeric() || character == '_' || character == '-');

    if pending_confirm {
        let current_name = world.resource::<MapInfo>().name.clone();
        ui.horizontal(|ui| {
            ui.colored_label(egui::Color32::RED, format!("Abandon '{current_name}' and create a new map?"));
            if ui.button("Yes").clicked() {
                create_new_map(world, &name, width, height);
            }
            if ui.button("Cancel").clicked() {
                world.resource_mut::<EditorState>().new_map_form.pending_confirm = false;
            }
        });
    } else {
        ui.add_enabled_ui(name_valid, |ui| {
            if ui.button("Create New Map").clicked() {
                world.resource_mut::<EditorState>().new_map_form.pending_confirm = true;
            }
        });
    }
}

fn tab_shards(ui: &mut egui::Ui, world: &mut World) {
    ui.heading("Shard Stock");
    ui.separator();

    egui::ScrollArea::vertical().max_height(300.0).show(ui, |ui| {
        for shard in Shard::all() {
            let count = world.resource::<Stock>().get(shard);
            ui.horizontal(|ui| {
                ui.label(format!("{shard}: {count}"));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button("−").clicked() {
                        world.resource_mut::<Stock>().try_remove((shard, 1));
                    }
                    if ui.button("+").clicked() {
                        world.resource_mut::<Stock>().add((shard, 1));
                    }
                });
            });
        }
    });
}
