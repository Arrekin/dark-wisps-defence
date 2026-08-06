use bevy::prelude::*;

use research::prelude::ResearchRuntime;
use research::research_bar::{BuilderResearchBar, ResearchBar};
use states::prelude::{GameState, UiInteraction};
use widgets::prelude::FillBar;

pub(crate) struct ResearchBarPlugin;
impl Plugin for ResearchBarPlugin {
    fn build(&self, app: &mut App) {
        app
            .add_observer(on_builder_add_spawn_research_bar)
            .add_systems(Update, sync_research_bars
                .run_if(in_state(GameState::Running))
                .run_if(in_state(UiInteraction::ResearchPanel)));
    }
}

/// Expands the `BuilderResearchBar` into the underlying `BuilderFillBar`
/// plus the runtime `ResearchBar` binding. The FillBar expansion observer
/// handles building the track + fill tree.
fn on_builder_add_spawn_research_bar(
    trigger: On<Add, BuilderResearchBar>,
    mut commands: Commands,
    builders: Query<&BuilderResearchBar>,
) {
    let entity = trigger.entity;
    let Ok(builder) = builders.get(entity) else { return };

    commands.entity(entity)
        .remove::<BuilderResearchBar>()
        .insert((
            builder.builder_fill_bar,
            builder.research_bar,
        ));
}

/// Writes `ResearchRuntime.progress` into each bar's `fill_fraction`. Gated
/// to the open panel — a bar never has to be correct while closed.
///
/// No `Changed<ResearchRuntime>` filter: progress accumulates while the
/// panel is closed, and a `Changed` filter would miss it because the change
/// fired on a frame the system didn't run.
fn sync_research_bars(
    runtimes: Query<&ResearchRuntime>,
    mut bars: Query<(&mut FillBar, &ResearchBar)>,
) {
    for (mut fill_bar, research_bar) in bars.iter_mut() {
        let Ok(runtime) = runtimes.get(research_bar.research) else { continue };
        if fill_bar.fill_fraction != runtime.progress {
            fill_bar.fill_fraction = runtime.progress;
        }
    }
}
