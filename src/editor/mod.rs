mod console;
mod objectives;
mod summonings;

use bevy_egui::{EguiContexts, EguiPrimaryContextPass, egui};
use strum::{AsRefStr, EnumIter, IntoEnumIterator};

use crate::objectives::{ObjectiveDetails, ObjectiveKillWisps};
use crate::prelude::*;
use crate::wisps::summoning::Summoning;

pub struct EditorPlugin;
impl Plugin for EditorPlugin {
    fn build(&self, app: &mut App) {
        app
            .add_plugins(bevy_egui::EguiPlugin::default())
            .insert_resource(bevy_egui::EguiGlobalSettings {
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
}

fn editor_ui(
    mut contexts: EguiContexts,
    mut state: ResMut<EditorState>,
    mut commands: Commands,
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
            }
        });
}

fn tab_general(ui: &mut egui::Ui) {
    ui.label("General settings");
}
