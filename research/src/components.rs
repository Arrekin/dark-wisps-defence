use std::time::Duration;

use bevy::prelude::*;
use game_core::prelude::MapBound;
use resources::prelude::Cost;
use strum::{AsRefStr, EnumString};

// ============================================================================
// Research entity
//
// A research is map content: it exists only on the map, enabled or not.
// Enablement is `ResearchState` being present — every query that fetches state
// automatically excludes disabled researches. A disabled research carries
// `Research`, `ContentId`, display, costs and outcomes, but no state and no
// runtime.
//
// `ResearchState` is the leading component: it is the entry point for
// enablement and progression. `ResearchRuntime` accompanies any state that
// can progress — `ResearchAvailable` and `ResearchActive` both require it,
// so an in-scenario research gets a default runtime whether never-started or
// parked. `ResearchCompleted` does not require it, and the tick removes it
// explicitly on completion. The require chain is indirect: inserting
// `ResearchState` fires the marker-swap observer, which inserts the matching
// marker; that marker requires `ResearchRuntime`, so a newly enabled research
// gets a default runtime. Loading may insert runtime first (with saved
// progress) — the require sees it already present and skips, so insert order
// does not matter.
// ============================================================================

#[derive(Component, Clone, Debug, Default)]
#[require(MapBound)]
pub struct Research {
    pub cost: Vec<Cost>,
    pub duration: Duration,
}

/// Progression axis and the single entry point for changing it. Inserting
/// fires `On<Insert, ResearchState>` which swaps the marker component below,
/// so queries keep archetype filters. Only enabled researches carry state.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq, Default, EnumString, AsRefStr, FromTemplate)]
#[component(immutable)]
pub enum ResearchState {
    #[default]
    Available,
    Active,
    Completed,
}

impl ResearchState {
    pub fn is_completed(self) -> bool {
        matches!(self, Self::Completed)
    }
}

// ---- Marker components (query-filter surface; never inserted directly) ----

/// Inserted by the marker-swap observer when `ResearchState::Available` is
/// inserted. Requires `ResearchRuntime`, so an Available research gets a
/// default progress component automatically.
#[derive(Component, Default)]
#[require(ResearchRuntime)]
pub struct ResearchAvailable;

/// Inserted by the marker-swap observer when `ResearchState::Active` is
/// inserted. Requires `ResearchRuntime` as `ResearchAvailable` does — both
/// are states that can progress.
#[derive(Component, Default)]
#[require(ResearchRuntime)]
pub struct ResearchActive;

/// Inserted by the marker-swap observer when `ResearchState::Completed` is
/// inserted. No runtime — completed researches have no progress.
#[derive(Component, Default)]
pub struct ResearchCompleted;

/// Marker on the research entity the player has selected for inspection in the
/// research panel. At most one research carries this at a time. Inserted and
/// removed by the tile-selection observer.
///
/// It is the selection itself, not a copy of it: the detail view bound to this
/// marker follows `On<Insert>` and `On<Remove>`, so it tracks the selection
/// without storing an `Entity` that could dangle. A selected research being
/// despawned removes the marker and empties the view.
#[derive(Component, Default)]
pub struct ResearchUISelected;

/// Progress data. Present on every enabled research via the
/// `ResearchAvailable` marker's require.
#[derive(Component, Clone, Debug, Default)]
pub struct ResearchRuntime {
    pub progress: f32,
}
