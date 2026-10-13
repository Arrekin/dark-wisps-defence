//! # Lob
//!
//! A `Lob` moves from origin to target at constant ground speed with a parabolic height curve.
//! Its duration and peak height are fixed at launch.
//!
//! Launchers use `LobProfile` to create flights. Launch angle and distance determine the peak
//! height, with a soft cap that flattens long shots. Gravity determines flight duration.

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
    /// The flight from `origin` to `target`.
    pub fn lob(&self, origin: Vec2, target: Vec2) -> Lob {
        let distance = origin.distance(target).max(1.);
        let uncapped_apex = distance * self.launch_angle.tan() / 4.;
        let apex = uncapped_apex / (1. + (uncapped_apex / self.max_apex).powf(APEX_CAP_HARDNESS)).powf(1. / APEX_CAP_HARDNESS);
        let duration = (8. * apex / self.gravity).sqrt();
        Lob { origin, target, duration, apex, elapsed: 0. }
    }
}

/// Shows a dotted flight preview while the tower is focused, using its `LobProfile`.
#[derive(Component, Default)]
pub struct LobPreview;

/// The flight of one lobbed projectile.
#[derive(Component, Clone, Copy, Debug)]
#[require(Visibility)]
pub struct Lob {
    origin: Vec2,
    target: Vec2,
    /// Seconds.
    duration: f32,
    /// Peak height above ground, in world units.
    apex: f32,
    /// Seconds since launch.
    elapsed: f32,
}
impl Lob {
    /// Rebuilds a flight in progress from its saved state.
    pub fn restore(origin: Vec2, target: Vec2, duration: f32, apex: f32, elapsed: f32) -> Self {
        Self { origin, target, duration, apex, elapsed }
    }

    pub fn origin(&self) -> Vec2 { self.origin }
    pub fn target(&self) -> Vec2 { self.target }
    pub fn duration(&self) -> f32 { self.duration }
    pub fn apex(&self) -> f32 { self.apex }
    pub fn elapsed(&self) -> f32 { self.elapsed }

    pub fn advance(&mut self, seconds: f32) {
        self.elapsed = (self.elapsed + seconds).min(self.duration);
    }

    pub fn is_landed(&self) -> bool {
        self.elapsed >= self.duration
    }

    /// Fraction of flight time elapsed: 0 at launch, 1 at landing.
    pub fn progress(&self) -> f32 {
        if self.duration <= 0. { return 1.; }
        (self.elapsed / self.duration).clamp(0., 1.)
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

    pub fn current(&self) -> LobSample {
        self.sample(self.progress())
    }

    /// Current speed relative to the launch speed, counting both ground and vertical motion.
    /// 1 at launch and landing, lowest at the apex.
    pub fn speed_ratio(&self) -> f32 {
        let distance = self.origin.distance(self.target);
        let launch = distance.hypot(4. * self.apex);
        if launch <= 0. { return 1.; }
        distance.hypot(4. * self.apex * (1. - 2. * self.progress())) / launch
    }
}

#[derive(Clone, Copy, Debug)]
pub struct LobSample {
    /// Point on the ground directly below the projectile.
    pub ground_position: Vec2,
    /// Height above ground, in world units.
    pub height: f32,
}
