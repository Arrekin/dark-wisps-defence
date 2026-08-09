//! A button carrying a line of text, for commands the player performs in one click.
//!
//! The button default is to fill provided space. Spawn your own [`Node`] alongside the builder to override
//! that.
//!
//! Clicking is not provided. Add your own `Pointer<Click>` on the button entity.

use bevy::prelude::*;

use crate::typography::TextRole;
use crate::void_panel::BuilderVoidPanel;

#[derive(Component, Clone, Copy, Debug)]
#[require(Button, Node = default_fill_center_node())]
pub struct TextCommandButton;

#[derive(Component, Clone, Copy, Debug)]
pub struct TextCommandButtonChildren {
    pub label: Entity,
}

#[derive(Component, Clone, Debug)]
pub struct BuilderTextCommandButton {
    pub text: String,
    pub text_role: TextRole,
    pub font_size: f32,
    pub text_color: Color,
    pub void_panel: BuilderVoidPanel,
}

impl BuilderTextCommandButton {
    pub fn new(text: impl Into<String>) -> Self {
        Self { text: text.into(), ..default() }
    }

    pub fn with_text(mut self, text: impl Into<String>) -> Self {
        self.text = text.into();
        self
    }

    pub fn with_text_role(mut self, role: TextRole) -> Self {
        self.text_role = role;
        self
    }

    pub fn with_font_size(mut self, size: f32) -> Self {
        self.font_size = size;
        self
    }

    pub fn with_text_color(mut self, color: impl Into<Color>) -> Self {
        self.text_color = color.into();
        self
    }

    /// Panel configuration for the button surface.
    pub fn with_void_panel(mut self, panel: BuilderVoidPanel) -> Self {
        self.void_panel = panel;
        self
    }
}

impl Default for BuilderTextCommandButton {
    fn default() -> Self {
        Self {
            text: String::new(),
            text_role: TextRole::Data,
            font_size: 12.0,
            text_color: Color::WHITE,
            // A control sits one step above the panel it is on: a tighter corner than a
            // full-size surface, and an edge bright enough to be found without hovering it.
            void_panel: BuilderVoidPanel::default()
                .with_corner_cut(5.0)
                .with_edge_brightness(0.6)
                .with_rim_intensity(0.12),
        }
    }
}

fn default_fill_center_node() -> Node {
    Node {
        width: Val::Percent(100.),
        height: Val::Percent(100.),
        justify_content: JustifyContent::Center,
        align_items: AlignItems::Center,
        ..default()
    }
}
