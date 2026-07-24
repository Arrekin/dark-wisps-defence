use bevy::prelude::*;

use game_core::prelude::moment_attach_self_trigger_to_parent;
use narrative::prelude::{
    MomentObjectiveFailed, MomentObjectiveSatisfied, ObjectiveFailedEvent,
    ObjectiveSatisfiedEvent,
};
use persistence::prelude::AppGameLoadSaveExtension;

pub(crate) struct ObjectiveMomentsPlugin;
impl Plugin for ObjectiveMomentsPlugin {
    fn build(&self, app: &mut App) {
        app
            .add_observer(moment_attach_self_trigger_to_parent::<MomentObjectiveSatisfied, ObjectiveSatisfiedEvent>)
            .add_observer(moment_attach_self_trigger_to_parent::<MomentObjectiveFailed, ObjectiveFailedEvent>)
            .register_moment_persistence::<MomentObjectiveSatisfied>()
            .register_moment_persistence::<MomentObjectiveFailed>()
            ;
    }
}
