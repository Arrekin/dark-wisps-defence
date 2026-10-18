//! Temporary visual entities whose lifetime follows the game clock.
//! Each instance stores its start time and duration. Shaders derive age from shared game time,
//! avoiding per-frame age updates. Register [`despawn_played_out`] to remove expired entities.

use bevy::prelude::*;

use session::GameClock;

use crate::lane::LaneVisual;

/// A timed lane visual on a disposable entity.
/// The cleanup system despawns the whole entity when its duration expires.
pub trait TransientVisual: LaneVisual {
    /// Start time from [`GameClock::wrapped`].
    fn start_time(&self) -> f32;
    /// Lifetime in seconds; must be shorter than the game clock's one-hour wrap period.
    fn duration(&self) -> f32;
}

/// Despawns every entity whose `K` has played out.
pub fn despawn_played_out<K: TransientVisual>(
    mut commands: Commands,
    clock: Res<GameClock>,
    visuals: Query<(Entity, &K)>,
) {
    for (entity, visual) in visuals.iter() {
        if clock.wrapped_since(visual.start_time()) >= visual.duration() {
            commands.entity(entity).despawn();
        }
    }
}
