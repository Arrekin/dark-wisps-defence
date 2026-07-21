pub(crate) mod objectives;

use bevy::prelude::*;

pub struct NarrativePlugin;
impl Plugin for NarrativePlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(objectives::ObjectivesPlugin);
    }
}
