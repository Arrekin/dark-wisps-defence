//! Shared session state: game time, entity lifetimes, statistics and the game-start moment.
//! `session_internal` owns updates, expiry, persistence and GPU clock uploads.

use bevy::prelude::*;
use serde::Serialize;

use game_core::prelude::{Moment, MomentKind, SSS};

/// Monotonic game-time counter in seconds. Advances only while `GameState::Running`.
///
/// Timed effects store absolute expiry timestamps relative to this clock.
#[derive(Resource, Default, SSS, Clone, Serialize)]
pub struct GameClock {
    /// Amount of game time passed since the session started, in seconds.
    /// Read it where the code measures a span. Use [`Self::now`] where it needs a point in time.
    pub elapsed: f64,
}
impl GameClock {
    /// Seconds after which the wrapped clock starts again at zero. Mirrors `GAME_TIME_WRAP_PERIOD`
    /// in `assets/shaders/common/game_time.wesl`.
    const WRAP_PERIOD: f64 = 3600.;

    /// The current moment on the game timeline, in seconds.
    /// Holds the same value as `elapsed` but reads as a point in time, not a span.
    /// Use it to stamp or compare timestamps.
    pub fn now(&self) -> f64 {
        self.elapsed
    }

    /// The game time `seconds` from now.
    pub fn from_now(&self, seconds: impl Into<f64>) -> f64 {
        self.elapsed + seconds.into()
    }

    /// Shader clock in seconds, wrapped to one hour before conversion to f32 to limit precision loss.
    /// Use [`Self::wrapped_since`] to measure durations shorter than the wrap period.
    pub fn wrapped(&self) -> f32 {
        Self::wrap(self.elapsed)
    }

    /// Wraps an absolute game time to the same one-hour range as [`Self::wrapped`].
    pub fn wrap(timestamp: f64) -> f32 {
        timestamp.rem_euclid(Self::WRAP_PERIOD) as f32
    }

    /// Seconds since a timestamp from [`Self::wrapped`], accounting for one clock wrap.
    /// Elapsed time must be shorter than one hour. Mirrors `game_time_since` in
    /// `assets/shaders/common/game_time.wesl`.
    pub fn wrapped_since(&self, timestamp: f32) -> f32 {
        (self.wrapped() - timestamp).rem_euclid(Self::WRAP_PERIOD as f32)
    }
}

/// An entity's lifetime in [`GameClock`] seconds. The entity despawns in `PostUpdate`
/// when the clock reaches `ends_at`.
///
/// Insert a new `TimeBound` to change the lifetime and reschedule despawn.
#[derive(Component, Clone, Copy, PartialEq, Debug)]
#[component(immutable)]
pub struct TimeBound {
    pub starts_at: f64,
    pub ends_at: f64,
}
impl TimeBound {
    /// Creates a lifetime starting now and lasting `duration` seconds.
    pub fn lasting(clock: &GameClock, duration: impl Into<f64>) -> Self {
        Self::new(clock.now(), clock.from_now(duration))
    }

    /// Creates a lifetime between two absolute [`GameClock`] timestamps.
    pub fn new(starts_at: f64, ends_at: f64) -> Self {
        Self { starts_at, ends_at }
    }

    /// Duration in seconds, clamped to zero if `ends_at` precedes `starts_at`.
    pub fn duration(&self) -> f64 {
        (self.ends_at - self.starts_at).max(0.)
    }

    /// Seconds since the start, clamped to `0..=duration`.
    pub fn age(&self, clock: &GameClock) -> f64 {
        (clock.now() - self.starts_at).clamp(0., self.duration())
    }

    /// Elapsed fraction of the lifetime, clamped to `0..=1`. Returns `1` if the duration is zero.
    pub fn progress(&self, clock: &GameClock) -> f64 {
        let duration = self.duration();
        if duration <= 0. { return 1.; }
        self.age(clock) / duration
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
