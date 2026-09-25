use bevy::prelude::*;

/// Where targeted feedback about a piece of work goes.
///
/// `None`: nobody asked for feedback. `Some(entity)`: trigger concrete, typed report events on
/// that entity. Events, builders and deferred work carry this value onward to whatever further
/// work should report back to the same recipient.
#[derive(Clone, Copy, Default, Debug)]
pub struct ResponseRequest(Option<Entity>);
impl ResponseRequest {
    /// Reports go to `target` via typed entity events.
    pub const fn new(target: Entity) -> Self {
        Self(Some(target))
    }

    /// No feedback wanted; reports go nowhere. Same value as `default()`, but names the intent.
    pub const fn not_needed() -> Self {
        Self(None)
    }

    /// Triggers the report built by `make` on the recipient; no-op when nobody asked.
    pub fn report<'t, E: EntityEvent<Trigger<'t>: Default>>(&self, commands: &mut Commands, make: impl FnOnce(Entity) -> E) {
        if let Some(target) = self.0 {
            commands.trigger(make(target));
        }
    }
}
