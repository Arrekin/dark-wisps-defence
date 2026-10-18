//! Shared session state: the game clock, statistics and game-start moment.
//! `session_internal` owns updates, persistence and GPU clock uploads.

use bevy::prelude::*;
use serde::Serialize;

use game_core::prelude::{Moment, MomentKind, SSS};

/// Monotonic game-time counter in seconds. Advances only while `GameState::Running`.
///
/// Timed effects store absolute expiry timestamps relative to this clock.
#[derive(Resource, Default, SSS, Clone, Serialize)]
pub struct GameClock {
    pub elapsed: f64,
}
impl GameClock {
    /// Seconds after which the wrapped clock starts again at zero. Mirrors `GAME_TIME_WRAP_PERIOD`
    /// in `assets/shaders/common/game_time.wesl`.
    const WRAP_PERIOD: f64 = 3600.;

    /// Shader clock in seconds, wrapped to one hour before conversion to f32 to limit precision loss.
    /// Use [`Self::wrapped_since`] to measure durations shorter than the wrap period.
    pub fn wrapped(&self) -> f32 {
        self.elapsed.rem_euclid(Self::WRAP_PERIOD) as f32
    }

    /// Seconds since a timestamp from [`Self::wrapped`], accounting for one clock wrap.
    /// Elapsed time must be shorter than one hour. Mirrors `game_time_since` in
    /// `assets/shaders/common/game_time.wesl`.
    pub fn wrapped_since(&self, timestamp: f32) -> f32 {
        (self.wrapped() - timestamp).rem_euclid(Self::WRAP_PERIOD as f32)
    }
}

/// The moment that starts a play session when the game enters `Running`.
/// Its `MomentOf` parent is itself, so it is a standalone moment.
#[derive(Component, Default, MomentKind)]
#[require(Moment, Name = Name::new("Game Start"))]
pub struct MomentGameStart;

/// Wisps killed in this play session.
#[derive(Resource, Default)]
pub struct StatsWispsKilled(pub usize);
