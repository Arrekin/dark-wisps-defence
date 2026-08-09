use bevy::prelude::*;

use research::prelude::{Research, ResearchRuntime};
use research::research_bar::{BuilderResearchBar, ResearchBar};
use resources::prelude::Stock;
use states::prelude::{GameState, UiInteraction};
use widgets::prelude::ProgressBar;

use crate::process::reachable_fraction;

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

/// Expands the `BuilderResearchBar` into the underlying `BuilderProgressBar`
/// plus the runtime `ResearchBar` binding. The ProgressBar expansion observer
/// handles creating the material asset.
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
            builder.builder_progress_bar,
            builder.research_bar,
            Node {
                width: Val::Percent(100.),
                height: Val::Percent(100.),
                ..default()
            },
        ));
}

/// Writes each bar's fraction, reachable mark and stall state from the research
/// it is bound to. Gated to the open panel — a bar never has to be correct
/// while closed.
///
/// No `Changed<ResearchRuntime>` filter: progress accumulates while the
/// panel is closed, and a `Changed` filter would miss it because the change
/// fired on a frame the system didn't run.
fn sync_research_bars(
    researches: Query<(&Research, &ResearchRuntime)>,
    stock: Res<Stock>,
    mut bars: Query<(&mut ProgressBar, &ResearchBar)>,
) {
    for (mut progress_bar, research_bar) in bars.iter_mut() {
        let Ok((research, runtime)) = researches.get(research_bar.research) else { continue };

        let reachable = reachable_fraction(runtime.progress, &research.cost, &stock);

        // Written through `bypass_change_detection` and marked by hand: this runs every
        // frame the panel is open, and only the active research's bar has anything new.
        // Marking all of them would rewrite every bar's material asset every frame.
        let bar = progress_bar.bypass_change_detection();
        let moved = bar.set_fraction(runtime.progress) | bar.set_reachable(reachable);
        if moved {
            progress_bar.set_changed();
        }
    }
}
