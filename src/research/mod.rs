//! # Research UI
//!
//! The research data model, mechanics, and persistence live in `lib_inventory`. This crate hosts
//! only the panel and its wiring (it needs `lib-ui`). See `feature_research.md` for the design.

use crate::prelude::*;

pub mod panel;

pub struct ResearchUiPlugin;
impl Plugin for ResearchUiPlugin {
    fn build(&self, app: &mut App) {
        panel::register(app);
    }
}
