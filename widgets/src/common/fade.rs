//! Shared fade types for shader-driven UI widgets.
//!
//! [`Fade`] is the GPU-side uniform: the endpoints, start time, and rate of an
//! exponential curve the shader evaluates against `globals.time`. Its layout
//! mirrors the `Fade` struct in the shaders that use it.
//!
//! [`FadeState`] is the runtime bookkeeping: the target a consumer asked for,
//! plus the fade carrying the drawn value toward it. It produces a [`Fade`] for
//! the material uniform.

use bevy::render::render_resource::ShaderType;

/// The GPU-side uniform: endpoints, start time, and rate of the exponential
/// curve. The shader evaluates it against `globals.time`. Layout mirrors the
/// `Fade` struct in the shaders that use it.
#[derive(ShaderType, Clone, Copy, Debug)]
pub struct Fade {
    pub start_value: f32,
    pub end_value: f32,
    /// Seconds on the same clock the shader reads from `globals.time`.
    pub start_time: f32,
    pub rate: f32,
}

/// Runtime bookkeeping behind a fade: the target a consumer asked for, plus the
/// fade currently carrying the drawn value toward it. Produces a [`Fade`] for
/// the material uniform.
///
/// Restarting from the currently sampled value is what lets a value reverse
/// mid-flight without a jump.
#[derive(Clone, Copy, Debug)]
pub(crate) struct FadeState {
    target: f32,
    start_value: f32,
    end_value: f32,
    start_time: f32,
    rate: f32,
}

impl FadeState {
    pub(crate) const fn new(rate: f32) -> Self {
        Self { target: 0.0, start_value: 0.0, end_value: 0.0, start_time: 0.0, rate }
    }

    /// Returns whether the target moved.
    pub(crate) fn set_target(&mut self, value: f32) -> bool {
        let moved = self.target != value;
        self.target = value;
        moved
    }

    /// Value at `now`, using the same curve as `eased()` in the shaders. Both
    /// definitions must be kept in step.
    pub(crate) fn sample(&self, now: f32) -> f32 {
        let elapsed = (now - self.start_time).max(0.0);
        self.start_value + (self.end_value - self.start_value) * (1.0 - (-self.rate * elapsed).exp())
    }

    /// Starts a fade toward the target, unless one is already headed there. The
    /// value at this instant becomes the new start, so interrupting a fade
    /// continues from where it visually was rather than snapping.
    pub(crate) fn begin_fade(&mut self, now: f32) {
        if self.target == self.end_value { return }
        self.start_value = self.sample(now);
        self.end_value = self.target;
        self.start_time = now;
    }

    pub(crate) fn fade(&self) -> Fade {
        Fade {
            start_value: self.start_value,
            end_value: self.end_value,
            start_time: self.start_time,
            rate: self.rate,
        }
    }
}
