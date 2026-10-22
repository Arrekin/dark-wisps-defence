//! # Lob
//!
//! A `Lob` moves from origin to target at constant ground speed with a parabolic height curve.
//! Its duration and peak height are fixed at launch.
//!
//! Launchers use `LobProfile` to create flights. Launch angle and distance determine the peak
//! height, with a soft cap that flattens long shots. Gravity determines flight duration.
//!
//! Flight progress is calculated from the current game time and a stored launch timestamp.
//! A lob can spawn before launch; `LobAwaitingLaunch` and `LobInFlight` distinguish waiting
//! projectiles from launched ones.

use bevy::prelude::*;

/// Sharpness of the transition from uncapped peak height to `max_apex`.
const APEX_CAP_HARDNESS: f32 = 4.;

/// Launch parameters used to create lobbed flights.
#[derive(Component, Clone, Copy, Debug)]
pub struct LobProfile {
    /// Launch angle in radians. Sets the uncapped apex: distance · tan(angle) / 4.
    pub launch_angle: f32,
    /// World units per second². Flight time = √(8 · apex / gravity).
    pub gravity: f32,
    /// Soft cap on the apex, in world units.
    pub max_apex: f32,
}
impl LobProfile {
    /// The flight from `origin` to `target`, launching at game time `launch_at`.
    pub fn lob(&self, origin: Vec2, target: Vec2, launch_at: f64) -> Lob {
        let distance = origin.distance(target).max(1.);
        let uncapped_apex = distance * self.launch_angle.tan() / 4.;
        let apex = uncapped_apex / (1. + (uncapped_apex / self.max_apex).powf(APEX_CAP_HARDNESS)).powf(1. / APEX_CAP_HARDNESS);
        let duration = (8. * apex / self.gravity).sqrt();
        Lob { origin, target, duration, apex, launch_at }
    }
}

/// Shows a dotted flight preview while the tower is focused, using its `LobProfile`.
#[derive(Component, Default)]
pub struct LobPreview;

/// A projectile's flight. The entity's `Transform` tracks its position on the ground.
#[derive(Component, Clone, Copy, Debug)]
#[require(Transform, Visibility)]
pub struct Lob {
    origin: Vec2,
    target: Vec2,
    /// Seconds.
    duration: f32,
    /// Peak height above ground, in world units.
    apex: f32,
    /// Absolute game-clock time of the launch, in seconds.
    launch_at: f64,
}
impl Lob {
    /// Rebuilds a flight from its saved state.
    pub fn restore(origin: Vec2, target: Vec2, duration: f32, apex: f32, launch_at: f64) -> Self {
        Self { origin, target, duration, apex, launch_at }
    }

    pub fn origin(&self) -> Vec2 { self.origin }
    pub fn target(&self) -> Vec2 { self.target }
    pub fn duration(&self) -> f32 { self.duration }
    pub fn apex(&self) -> f32 { self.apex }
    pub fn launch_at(&self) -> f64 { self.launch_at }

    /// Whether `now` is before the scheduled launch time.
    pub fn is_awaiting_launch(&self, now: f64) -> bool {
        self.launch_at > now
    }

    /// Seconds of flight at game time `now`: 0 before launch, `duration` after landing.
    pub fn elapsed(&self, now: f64) -> f32 {
        ((now - self.launch_at) as f32).clamp(0., self.duration)
    }

    pub fn is_landed(&self, now: f64) -> bool {
        self.elapsed(now) >= self.duration
    }

    /// Fraction of flight time elapsed at game time `now`: 0 at launch, 1 at landing.
    pub fn progress(&self, now: f64) -> f32 {
        if self.duration <= 0. { return 1.; }
        self.elapsed(now) / self.duration
    }

    /// Unit direction of travel over the ground.
    pub fn heading(&self) -> Vec2 {
        (self.target - self.origin).normalize_or(Vec2::X)
    }

    /// The flight's state at `progress` (0 = launch, 1 = landing).
    pub fn sample(&self, progress: f32) -> LobSample {
        LobSample {
            ground_position: self.origin.lerp(self.target, progress),
            height: 4. * self.apex * progress * (1. - progress),
        }
    }

    /// The flight's state at game time `now`.
    pub fn current(&self, now: f64) -> LobSample {
        self.sample(self.progress(now))
    }

    /// Speed at game time `now` relative to the launch speed, counting both ground and vertical
    /// motion. 1 at launch and landing, lowest at the apex.
    pub fn speed_ratio(&self, now: f64) -> f32 {
        let distance = self.origin.distance(self.target);
        let launch = distance.hypot(4. * self.apex);
        if launch <= 0. { return 1.; }
        distance.hypot(4. * self.apex * (1. - 2. * self.progress(now))) / launch
    }
}

/// A projectile waiting for its launch time, hidden at its origin.
#[derive(Component, Default)]
pub struct LobAwaitingLaunch;

/// A launched projectile, visible and moving along its flight.
#[derive(Component, Default)]
pub struct LobInFlight;

#[derive(Clone, Copy, Debug)]
pub struct LobSample {
    /// Point on the ground directly below the projectile.
    pub ground_position: Vec2,
    /// Height above ground, in world units.
    pub height: f32,
}
