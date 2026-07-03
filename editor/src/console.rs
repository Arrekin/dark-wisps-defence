//! # Log Console
//!
//! An egui window that displays entries from the in-game [`LogBuffer`].
//! Toggled with the backtick key. When egui has keyboard focus,
//! `absorb_bevy_input_system` clears all Bevy key state before `Update` runs,
//! so the run condition never fires and game shortcuts are blocked.
//! Available to both players and developers; filter controls narrow entries
//! by level, audience, tag, and free-text search.
//!
//! Uses `ScrollArea::show_rows` so only visible rows are rendered regardless
//! of how many entries are in the buffer.
//!
//! Tag filter: empty selection = no tag filter; selecting tags shows only
//! entries that share at least one tag with the selection.

use std::time::{SystemTime, UNIX_EPOCH};

use bevy::{input::common_conditions::input_just_pressed, platform::collections::HashSet, prelude::*};
use bevy_egui::{EguiContexts, EguiPrimaryContextPass, egui};
use strum::IntoEnumIterator;

use logging::{Audience, LogBuffer, LogEntryData, LogLevel, prelude::*};

const TIME_COL_WIDTH: f32     = 70.0;
const LEVEL_COL_WIDTH: f32    = 48.0;
const AUDIENCE_COL_WIDTH: f32 = 58.0;
const TAGS_COL_WIDTH: f32     = 160.0;

pub struct LogConsolePlugin;
impl Plugin for LogConsolePlugin {
    fn build(&self, app: &mut App) {
        app
            .init_resource::<LogConsoleState>()
            .add_systems(Update, (
                LogConsoleState::toggle_visibility.run_if(input_just_pressed(KeyCode::Backquote)),
            ))
            .add_systems(EguiPrimaryContextPass, LogConsoleState::render)
            ;
    }
}

// ── State ─────────────────────────────────────────────────────────────────────

#[derive(Resource)]
pub struct LogConsoleState {
    visible: bool,
    show_debug: bool,
    show_info: bool,
    show_warn: bool,
    show_error: bool,
    show_developer: bool,
    show_player: bool,
    active_tags: HashSet<Tag>,  // empty = no tag filter
    text_filter: String,
    auto_scroll: bool,
}
impl Default for LogConsoleState {
    fn default() -> Self {
        Self {
            visible: false,
            show_debug: true,
            show_info: true,
            show_warn: true,
            show_error: true,
            show_developer: true,
            show_player: true,
            active_tags: HashSet::new(),
            text_filter: String::new(),
            auto_scroll: true,
        }
    }
}
impl LogConsoleState {
    fn matches_filter(&self, entry: &LogEntryData, text_filter_lower: &str) -> bool {
        let level_ok = match entry.level {
            LogLevel::Debug => self.show_debug,
            LogLevel::Info  => self.show_info,
            LogLevel::Warn  => self.show_warn,
            LogLevel::Error => self.show_error,
        };
        let audience_ok = match entry.audience {
            Audience::Developer => self.show_developer,
            Audience::Player    => self.show_player,
        };
        let tag_ok = self.active_tags.is_empty() || !entry.tags.is_disjoint(&self.active_tags);
        let text_ok = text_filter_lower.is_empty() || entry.message.to_lowercase().contains(text_filter_lower);
        level_ok && audience_ok && tag_ok && text_ok
    }

    fn toggle_visibility(mut state: ResMut<Self>) {
        state.visible = !state.visible;
    }

    fn render(
        mut contexts: EguiContexts,
        log_buffer: Res<LogBuffer>,
        mut state: ResMut<Self>,
    ) {
        if !state.visible { return; }
        let Ok(ctx) = contexts.ctx_mut() else { return };

        let mut window_open = state.visible;
        egui::Window::new("Log Console")
            .open(&mut window_open)
            .resizable(true)
            .default_size([900.0, 350.0])
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label("Level:");
                    ui.toggle_value(&mut state.show_debug, "Debug");
                    ui.toggle_value(&mut state.show_info, "Info");
                    ui.toggle_value(&mut state.show_warn, "Warn");
                    ui.toggle_value(&mut state.show_error, "Error");
                    ui.separator();
                    ui.label("Audience:");
                    ui.toggle_value(&mut state.show_developer, "Dev");
                    ui.toggle_value(&mut state.show_player, "Player");
                    ui.separator();
                    ui.add(
                        egui::TextEdit::singleline(&mut state.text_filter)
                            .hint_text("Filter...")
                            .desired_width(120.0),
                    );
                    ui.separator();
                    ui.toggle_value(&mut state.auto_scroll, "Auto-scroll");
                });

                ui.horizontal(|ui| {
                    ui.label("Tags:");
                    for tag in Tag::iter() {
                        let mut active = state.active_tags.contains(&tag);
                        if ui.toggle_value(&mut active, tag.as_ref()).changed() {
                            if active { state.active_tags.insert(tag); }
                            else      { state.active_tags.remove(&tag); }
                        }
                    }
                });

                ui.separator();

                let row_height = ui.text_style_height(&egui::TextStyle::Body);
                ui.horizontal(|ui| {
                    ui.add_sized([TIME_COL_WIDTH, row_height],     egui::Label::new(egui::RichText::new("Time").strong()));
                    ui.add_sized([LEVEL_COL_WIDTH, row_height],    egui::Label::new(egui::RichText::new("Level").strong()));
                    ui.add_sized([AUDIENCE_COL_WIDTH, row_height], egui::Label::new(egui::RichText::new("Audience").strong()));
                    ui.add_sized([TAGS_COL_WIDTH, row_height],     egui::Label::new(egui::RichText::new("Tags").strong()));
                    ui.label(egui::RichText::new("Message").strong());
                });
                ui.separator();

                let text_filter_lower = state.text_filter.to_lowercase();
                let filtered: Vec<&LogEntryData> = log_buffer.entries()
                    .filter(|entry| state.matches_filter(entry, &text_filter_lower))
                    .collect();

                let auto_scroll = state.auto_scroll;
                egui::ScrollArea::vertical()
                    .auto_shrink(false)
                    .stick_to_bottom(auto_scroll)
                    .show_rows(ui, row_height, filtered.len(), |ui, row_range| {
                        ui.set_width(ui.available_width());
                        for index in row_range {
                            let entry = filtered[index];
                            let color = entry.level.egui_color();
                            ui.horizontal(|ui| {
                                ui.add_sized([TIME_COL_WIDTH, row_height],
                                    egui::Label::new(egui::RichText::new(format_timestamp(entry.timestamp)).color(color)).truncate()
                                );
                                ui.add_sized([LEVEL_COL_WIDTH, row_height],
                                    egui::Label::new(egui::RichText::new(format!("{}", entry.level)).color(color)).truncate()
                                );
                                ui.add_sized([AUDIENCE_COL_WIDTH, row_height],
                                    egui::Label::new(egui::RichText::new(format!("{}", entry.audience)).color(color)).truncate()
                                );
                                ui.add_sized([TAGS_COL_WIDTH, row_height],
                                    egui::Label::new(egui::RichText::new(entry.tags_sorted_as_string()).color(color)).truncate()
                                );
                                ui.add(
                                    egui::Label::new(egui::RichText::new(entry.message.as_ref()).color(color)).truncate()
                                );
                            });
                        }
                    });
            });

        state.visible = window_open;
    }
}

// ── LogLevel egui extension ───────────────────────────────────────────────────

trait LogLevelColor {
    fn egui_color(self) -> egui::Color32;
}
impl LogLevelColor for LogLevel {
    fn egui_color(self) -> egui::Color32 {
        match self {
            LogLevel::Debug => egui::Color32::from_gray(140),
            LogLevel::Info  => egui::Color32::WHITE,
            LogLevel::Warn  => egui::Color32::from_rgb(255, 200, 0),
            LogLevel::Error => egui::Color32::from_rgb(255, 80, 80),
        }
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn format_timestamp(time: SystemTime) -> String {
    let total_seconds = time.duration_since(UNIX_EPOCH).unwrap_or_default().as_secs();
    let hours   = (total_seconds / 3600) % 24;
    let minutes = (total_seconds / 60) % 60;
    let seconds = total_seconds % 60;
    format!("{hours:02}:{minutes:02}:{seconds:02}")
}
