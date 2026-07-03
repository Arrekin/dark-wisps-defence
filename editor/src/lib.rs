mod console;
mod objectives;
mod summonings;

use bevy::prelude::*;
use bevy_egui::{EguiContexts, EguiPrimaryContextPass, egui};
use strum::{AsRefStr, EnumIter, IntoEnumIterator};

use game_core::prelude::ShardType;
use narrative::prelude::{ObjectiveDetails, ObjectiveKillWisps};
use shards::prelude::*;
use states::AdminMode;
use wisps::summoning::Summoning;

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
}

#[derive(Default, Clone, Copy, PartialEq, Eq, EnumIter, AsRefStr)]
pub enum EditorTab {
    #[default]
    General,
    Summonings,
    Objectives,
    Shards,
}

fn editor_ui(
    mut commands: Commands,
    mut state: ResMut<EditorState>,
    mut contexts: EguiContexts,
    mut inventory: ResMut<ShardInventory>,
    mut summonings: Query<(Entity, &mut Summoning)>,
    mut objectives_query: Query<(Entity, &mut ObjectiveDetails, Option<&mut ObjectiveKillWisps>)>,
) {
    let Ok(ctx) = contexts.ctx_mut() else { return };
    egui::Window::new("Editor")
        .resizable(true)
        .default_width(400.0)
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                for tab in EditorTab::iter() {
                    if ui.selectable_label(state.active_tab == tab, tab.as_ref()).clicked() {
                        state.active_tab = tab;
                    }
                }
            });

            ui.separator();

            match state.active_tab {
                EditorTab::General => tab_general(ui),
                EditorTab::Summonings => summonings::tab_summonings(ui, &mut state, &mut commands, &mut summonings),
                EditorTab::Objectives => objectives::tab_objectives(ui, &mut state, &mut commands, &mut objectives_query),
                EditorTab::Shards => tab_shards(ui, &mut inventory),
            }
        });
}

fn tab_general(ui: &mut egui::Ui) {
    ui.label("General settings");
}

fn tab_shards(ui: &mut egui::Ui, inventory: &mut ResMut<ShardInventory>) {
    ui.heading("Shard Inventory");

    ui.horizontal(|ui| {
        ui.menu_button("+ Add Shard", |ui| {
            for shard_type in ShardType::iter() {
                if ui.button(shard_type.to_string()).clicked() {
                    inventory.add(shard_type, 1);
                    ui.close();
                }
            }
        });
    });

    ui.separator();

    let shards: Vec<(ShardType, usize)> = inventory.iter().collect();

    if shards.is_empty() {
        ui.label("No shards in inventory");
    } else {
        egui::ScrollArea::vertical().max_height(300.0).show(ui, |ui| {
            for (shard_type, count) in shards {
                ui.horizontal(|ui| {
                    ui.label(format!("{}: {}", shard_type, count));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button("−").clicked() {
                            if inventory.has(shard_type) {
                                inventory.remove(shard_type);
                            }
                        }
                        if ui.button("+").clicked() {
                            inventory.add(shard_type, 1);
                        }
                    });
                });
            }
        });
    }
}
