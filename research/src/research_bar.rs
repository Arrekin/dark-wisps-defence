use bevy::prelude::*;

use widgets::prelude::BuilderProgressBar;

/// Runtime component on the ProgressBar entity. Stores which research this bar
/// reflects, so `sync_research_bars` can look up `ResearchRuntime.progress`
/// without walking any tree.
#[derive(Component, Clone, Copy, Debug)]
pub struct ResearchBar {
    pub research: Entity,
}

/// Spawn contract for a research progress bar. Carries the research entity
/// and the underlying `BuilderProgressBar` so the caller controls the bar's
/// appearance through the same builder surface.
#[derive(Component, Clone, Copy, Debug)]
pub struct BuilderResearchBar {
    pub research_bar: ResearchBar,
    pub builder_progress_bar: BuilderProgressBar,
}

impl BuilderResearchBar {
    pub fn new(research: Entity) -> Self {
        Self::default().with_research(research)
    }

    pub fn with_research(mut self, research: Entity) -> Self {
        self.research_bar.research = research;
        self
    }

    pub fn with_fraction(mut self, fraction: f32) -> Self {
        self.builder_progress_bar = self.builder_progress_bar.with_fraction(fraction);
        self
    }

    pub fn with_progress_bar(mut self, builder: BuilderProgressBar) -> Self {
        self.builder_progress_bar = builder;
        self
    }
}

impl Default for BuilderResearchBar {
    fn default() -> Self {
        Self {
            research_bar: ResearchBar { research: Entity::PLACEHOLDER },
            builder_progress_bar: BuilderProgressBar::default(),
        }
    }
}
