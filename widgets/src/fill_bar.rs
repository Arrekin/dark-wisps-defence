use bevy::prelude::*;

/// Which way the fill grows: horizontal from the left, or vertical from the
/// bottom. Decides both how the fraction is applied and which way the fill
/// extends.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum FillAxis {
    #[default]
    Horizontal,
    Vertical,
}

/// Runtime component a caller writes. `fill_fraction` is 0..=1. The sync
/// system reads it on change and writes the fill node's size.
#[derive(Component, Clone, Copy, Debug, Default)]
pub struct FillBar {
    pub fill_fraction: f32,
    pub axis: FillAxis,
}

/// Holds the entities of the nodes `FillBar` spawns.
/// `fill` is the inner node whose size is driven by `fill_fraction`.
#[derive(Component, Clone, Copy, Debug)]
pub struct FillBarChildren {
    pub fill: Entity,
}

/// Spawn contract for a fill bar. Its fields are the whole customisation
/// surface: axis, track size, colours, border. The widget builds the tree
/// (track + fill) and records the fill entity — nothing is searched for at
/// runtime.
#[derive(Component, Clone, Copy, Debug)]
pub struct BuilderFillBar {
    pub fill_bar: FillBar,
    pub background_color: BackgroundColor,
    pub border_color: BorderColor,
    pub border: UiRect,
    pub border_radius: BorderRadius,
    pub fill_color: BackgroundColor,
}

impl BuilderFillBar {
    pub fn with_axis(mut self, axis: FillAxis) -> Self {
        self.fill_bar.axis = axis;
        self
    }

    pub fn with_fill_fraction(mut self, fraction: f32) -> Self {
        self.fill_bar.fill_fraction = fraction;
        self
    }

    pub fn with_background_color(mut self, color: impl Into<Color>) -> Self {
        self.background_color = BackgroundColor::from(color.into());
        self
    }

    pub fn with_border(mut self, color: impl Into<Color>, border: UiRect) -> Self {
        self.border_color = BorderColor::from(color.into());
        self.border = border;
        self
    }

    pub fn with_border_radius(mut self, radius: BorderRadius) -> Self {
        self.border_radius = radius;
        self
    }

    pub fn with_fill_color(mut self, color: impl Into<Color>) -> Self {
        self.fill_color = BackgroundColor::from(color.into());
        self
    }
}

impl Default for BuilderFillBar {
    fn default() -> Self {
        Self {
            fill_bar: FillBar::default(),
            background_color: BackgroundColor::from(Color::srgba(0.1, 0.1, 0.1, 0.8)),
            border_color: BorderColor::from(Color::srgba(0.4, 0.4, 0.4, 1.)),
            border: UiRect::all(Val::Px(1.0)),
            border_radius: BorderRadius::ZERO,
            fill_color: BackgroundColor::from(Color::srgba(0.2, 0.6, 1.0, 1.0)),
        }
    }
}
