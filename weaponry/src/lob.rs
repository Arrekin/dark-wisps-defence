//! # Lob
//!
//! Parabolic projectile trajectories ([`LobArc`]), flight progression ([`Lob`], [`LobArcProgress`]),
//! and trajectory preview state ([`LobPreview`], [`LobPreviewScope`], [`LobPreviewShown`]).
//!
//! Launchers construct [`LobArc`]s through a [`LobProfile`], which derives peak height from launch
//! angle and horizontal distance (with a soft cap at `max_apex`) and flight duration from gravity.
//!
//! Adding [`Lob`] to an entity advances its [`LobArcProgress`] and ground [`Transform`] each frame
//! and triggers [`LobLanded`] on arrival.

use bevy::prelude::*;

/// Sharpness of the transition from uncapped peak height to `max_apex`.
const APEX_CAP_HARDNESS: f32 = 4.;

/// Ballistic parameters used by a launcher to construct [`LobArc`] trajectories.
#[derive(Component, Clone, Copy, Debug)]
pub struct LobProfile {
    /// Launch angle in radians. Sets the uncapped apex: `distance * tan(angle) / 4`.
    pub launch_angle: f32,
    /// Downward acceleration in world units/s². Sets flight duration: `sqrt(8 * apex / gravity)`.
    pub gravity: f32,
    /// Soft cap on peak height, in world units.
    pub max_apex: f32,
}
impl LobProfile {
    /// Computes the trajectory from `origin` to `target`.
    pub fn arc(&self, origin: Vec2, target: Vec2) -> LobArc {
        let distance = origin.distance(target).max(1.);
        let uncapped_apex = distance * self.launch_angle.tan() / 4.;
        let apex = uncapped_apex / (1. + (uncapped_apex / self.max_apex).powf(APEX_CAP_HARDNESS)).powf(1. / APEX_CAP_HARDNESS);
        let duration = (8. * apex / self.gravity).sqrt();
        LobArc::new(origin, target, duration, apex)
    }
}

/// Parabolic trajectory between `origin` and `target` with fixed `duration` and peak height `apex`.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
#[component(immutable)]
#[require(LobArcProgress)]
pub struct LobArc {
    pub origin: Vec2,
    pub target: Vec2,
    /// Total flight time in seconds.
    pub duration: f32,
    /// Peak height above ground, in world units.
    pub apex: f32,
}
impl LobArc {
    pub fn new(origin: Vec2, target: Vec2, duration: f32, apex: f32) -> Self {
        Self { origin, target, duration, apex }
    }

    /// Fraction of the trajectory covered in `seconds`.
    pub fn progress_over(&self, seconds: f32) -> f32 {
        if self.duration <= 0. { return 1.; }
        seconds / self.duration
    }

    /// Unit ground-plane direction from `origin` toward `target`.
    pub fn heading(&self) -> Vec2 {
        (self.target - self.origin).normalize_or(Vec2::X)
    }

    /// Ground position and height at normalized `progress` (`0.0` at origin, `1.0` at target).
    pub fn sample(&self, progress: f32) -> LobSample {
        LobSample {
            ground_position: self.origin.lerp(self.target, progress),
            height: 4. * self.apex * progress * (1. - progress),
        }
    }

    /// 3D speed at `progress` normalized to the launch/landing speed (`1.0` at endpoints, minimum at
    /// the apex).
    pub fn speed_ratio(&self, progress: f32) -> f32 {
        let distance = self.origin.distance(self.target);
        let launch = distance.hypot(4. * self.apex);
        if launch <= 0. { return 1.; }
        distance.hypot(4. * self.apex * (1. - 2. * progress)) / launch
    }

    /// Linearly interpolates all trajectory parameters between `self` (`t = 0.0`) and `other` (`t = 1.0`).
    pub fn lerp(&self, other: &LobArc, t: f32) -> LobArc {
        LobArc {
            origin: self.origin.lerp(other.origin, t),
            target: self.target.lerp(other.target, t),
            duration: self.duration.lerp(other.duration, t),
            apex: self.apex.lerp(other.apex, t),
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct LobSample {
    /// Point on the ground directly below the projectile.
    pub ground_position: Vec2,
    /// Height above ground, in world units.
    pub height: f32,
}

/// Normalized progress along a [`LobArc`], from `0.0` (origin) to `1.0` (target).
#[derive(Component, Clone, Copy, Default, Debug, PartialEq)]
pub struct LobArcProgress(pub f32);
impl LobArcProgress {
    pub fn is_at_target(&self) -> bool {
        self.0 >= 1.
    }

    /// Advances progress by `fraction`, clamping at `1.0`.
    pub fn advance(&mut self, fraction: f32) {
        self.0 = (self.0 + fraction).min(1.);
    }
}

/// Marker for an active lob flight. Advances [`LobArcProgress`] and [`Transform`] along [`LobArc`]
/// each frame and triggers [`LobLanded`] on arrival.
#[derive(Component, Default)]
#[require(Transform)]
pub struct Lob;

/// Triggered on a [`Lob`] entity when its [`LobArcProgress`] reaches `1.0`.
#[derive(EntityEvent)]
pub struct LobLanded(pub Entity);

/// Opts an entity into trajectory preview rendering when covered by [`LobPreviewScope`].
#[derive(Component, Default)]
pub struct LobPreview;

/// Controls which [`LobPreview`] entities display their trajectory preview.
#[derive(Resource, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum LobPreviewScope {
    #[default]
    None,
    Global,
    Entity(Entity),
}
impl LobPreviewScope {
    pub fn covers(&self, entity: Entity) -> bool {
        match self {
            LobPreviewScope::None => false,
            LobPreviewScope::Global => true,
            LobPreviewScope::Entity(scoped) => *scoped == entity,
        }
    }
}

/// Added to [`LobPreview`] entities currently covered by [`LobPreviewScope`].
#[derive(Component, Default)]
pub struct LobPreviewShown {
    /// Currently displayed trajectory. Snaps to [`LobArc`] when initialized from `None`, eases
    /// toward subsequent [`LobArc`] changes, and resets to `None` when [`LobArc`] is removed.
    pub arc: Option<LobArc>,
}

/// Syncs [`LobPreviewShown`] with [`LobPreviewScope`] and eases displayed arcs toward [`LobArc`].
/// Systems updating [`LobArc`] should run before this set.
#[derive(SystemSet, Debug, Hash, PartialEq, Eq, Clone)]
pub struct LobPreviewSystems;
