use bevy::prelude::*;

use widgets::prelude::BuilderFillBar;

/// Runtime component on the FillBar entity. Stores which research this bar
/// reflects, so `sync_research_bars` can look up `ResearchRuntime.progress`
/// without walking any tree.
#[derive(Component, Clone, Copy, Debug)]
pub struct ResearchBar {
    pub research: Entity,
}

/// Spawn contract for a research progress bar. Carries the research entity
/// and the underlying `BuilderFillBar` so the caller controls the bar's
/// appearance through the same builder surface.
#[derive(Component, Clone, Debug)]
pub struct BuilderResearchBar {
    pub research_bar: ResearchBar,
    pub builder_fill_bar: BuilderFillBar,
}

impl BuilderResearchBar {
    pub fn new(research: Entity) -> Self {
        Self::default().with_research(research)
    }

    pub fn with_research(mut self, research: Entity) -> Self {
        self.research_bar.research = research;
        self
    }

    pub fn with_fill_fraction(mut self, fraction: f32) -> Self {
        self.builder_fill_bar = self.builder_fill_bar.with_fill_fraction(fraction);
        self
    }

    pub fn with_fill_bar(mut self, builder: BuilderFillBar) -> Self {
        self.builder_fill_bar = builder;
        self
    }
}

impl Default for BuilderResearchBar {
    fn default() -> Self {
        Self {
            research_bar: ResearchBar { research: Entity::PLACEHOLDER },
            builder_fill_bar: BuilderFillBar::default()
                .with_background_color(Color::linear_rgba(0.1, 0.1, 0.1, 0.8))
                .with_border(Color::linear_rgba(0.4, 0.4, 0.4, 1.), UiRect::all(Val::Px(1.)))
                .with_fill_color(Color::linear_rgba(0.3, 0.6, 1.0, 1.0)),
        }
    }
}
