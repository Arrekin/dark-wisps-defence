use bevy::prelude::*;

use crate::model::ResearchType;

/// Fired when a research finishes. Reactors (e.g. the panel) may observe it.
#[derive(Event)]
pub struct ResearchCompleted(pub ResearchType);

/// Start a research, or switch the active slot to it. Parks any currently-active research (progress
/// retained). No-op if the research is unknown, already completed, or obsolete (its outputs owned).
#[derive(Event)]
pub struct SetActiveResearch(pub ResearchType);

/// Park the active research (progress retained). There is no cancel — progress is never destroyed.
#[derive(Event)]
pub struct StopResearch;
