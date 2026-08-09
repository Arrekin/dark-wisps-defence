//! # Research Detail View
//!
//! The expanded card showing one research in full. [`view`] builds and maintains it;
//! [`active`] adds what only the card following the running research does.
//!
//! Everything a card is made of stays inside this module tree. The panel that hosts the
//! cards spawns them through [`BuilderResearchDetailView`] and binds each to a research
//! marker with [`ResearchDetailViewSource`]; it needs nothing else, and nothing else in the
//! crate reaches into a card's parts.

use bevy::prelude::*;

pub(crate) mod active;
pub(crate) mod view;

pub(crate) use view::{BuilderResearchDetailView, ResearchDetailViewSource};

/// The card and its live behaviour. Split so that the shell — layout, subject binding,
/// rebuilds — stays readable next to the effects that only one card ever shows.
pub(crate) struct ResearchDetailViewPlugin;
impl Plugin for ResearchDetailViewPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            view::DetailViewShellPlugin,
            active::ActiveDetailViewPlugin,
        ));
    }
}
