use bevy::color::palettes::css::GREEN;
use bevy::prelude::*;

use crate::fill_bar::BuilderFillBar;

/// Runtime component a caller writes. `value` and `max_value` drive the
/// fill fraction; `color` drives the fill colour. All three are read by the
/// sync system on change.
#[derive(Component, Clone, Copy, Debug)]
pub struct Healthbar {
    pub value: f32,
    pub max_value: f32,
    pub color: Color,
}

impl Default for Healthbar {
    fn default() -> Self {
        Self { value: 0., max_value: 0., color: GREEN.into() }
    }
}

impl Healthbar {
    pub fn get_fraction(&self) -> f32 {
        if self.max_value == 0. { 1. } else { self.value / self.max_value }
    }
}

/// Spawn contract for a healthbar. Carries the initial runtime values and
/// the underlying `BuilderFillBar` so the caller controls the bar's
/// appearance through the same builder surface.
#[derive(Component, Clone, Debug)]
pub struct BuilderHealthbar {
    pub healthbar: Healthbar,
    pub builder_fill_bar: BuilderFillBar,
    pub font_size: f32,
}

impl BuilderHealthbar {
    pub fn with_value(mut self, value: f32) -> Self {
        self.healthbar.value = value;
        self
    }

    pub fn with_max_value(mut self, max_value: f32) -> Self {
        self.healthbar.max_value = max_value;
        self
    }

    pub fn with_color(mut self, color: impl Into<Color>) -> Self {
        self.healthbar.color = color.into();
        self
    }

    pub fn with_font_size(mut self, font_size: f32) -> Self {
        self.font_size = font_size;
        self
    }

    pub fn with_fill_bar(mut self, builder: BuilderFillBar) -> Self {
        self.builder_fill_bar = builder;
        self
    }
}

impl Default for BuilderHealthbar {
    fn default() -> Self {
        Self {
            healthbar: Healthbar::default(),
            builder_fill_bar: BuilderFillBar::default()
                .with_background_color(Color::linear_rgba(0., 0., 0., 0.))
                .with_border(Color::linear_rgba(0., 0.2, 1., 1.), UiRect::all(Val::Px(2.0))),
            font_size: 16.,
        }
    }
}
