pub(crate) mod objectives;
pub(crate) mod panel;

use bevy::prelude::*;

pub struct NarrativePlugin;
impl Plugin for NarrativePlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            objectives::ObjectivesPlugin,
            panel::ObjectivesPanelPlugin,
        ));
    }
}
