//! Font selection by semantic text role.
//!
//! Fonts are resolved by their embedded family names rather than file paths so upright and italic
//! variants remain available to the text system.

use bevy::prelude::*;

#[derive(Clone, Copy, Debug)]
pub enum TextRole {
    /// Space Grotesk. Short headings, often uppercase.
    Heading,
    /// Inter. Everything read as prose.
    Body,
    /// JetBrains Mono. Numbers, costs, timers — anything that should not reflow as its
    /// value changes.
    Data,
}

impl TextRole {
    fn family(self) -> &'static str {
        match self {
            Self::Heading => "Space Grotesk",
            Self::Body => "Inter",
            Self::Data => "JetBrains Mono",
        }
    }

    /// Roles carry a weight rather than inheriting each file's default instance. Space Grotesk's
    /// default is Light, which on a heading reads as a mistake.
    fn weight(self) -> FontWeight {
        match self {
            Self::Heading => FontWeight::SEMIBOLD,
            Self::Body | Self::Data => FontWeight::NORMAL,
        }
    }

    pub fn font(self, size: f32) -> TextFont {
        TextFont {
            font: FontSource::Family(self.family().into()),
            font_size: FontSize::Px(size),
            weight: self.weight(),
            ..default()
        }
    }
}
