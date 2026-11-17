//! # Shader clocks
//!
//! Shaders read two clocks: Bevy's `Time` (`globals.time`) and [`session::GameClock`]. Both wrap to
//! zero every [`CLOCK_WRAP_PERIOD`] seconds so their `f32` values retain precision over long
//! sessions. [`ClockInstant`] pairs the wrapped seconds with the wrap count (`epoch`) so elapsed
//! time between instants remains exact across wraps.

use bevy::{prelude::*, render::render_resource::ShaderType};
use bytemuck::{Pod, Zeroable};

/// Wrap period in seconds shared by both shader clocks. Mirrors `CLOCK_WRAP_PERIOD` in
/// `assets/shaders/common/clock.wesl`.
pub const CLOCK_WRAP_PERIOD: f64 = 3600.;

/// Timestamp on a wrapped shader clock.
/// Field order and types mirror `ClockInstant` in `assets/shaders/common/clock.wesl`.
#[derive(Clone, Copy, PartialEq, Debug, Default, Pod, Zeroable, ShaderType)]
#[repr(C)]
pub struct ClockInstant {
    /// Completed [`CLOCK_WRAP_PERIOD`] intervals before this instant.
    pub epoch: u32,
    /// Elapsed seconds within the current wrap period (`0.0..CLOCK_WRAP_PERIOD`).
    pub seconds: f32,
}
impl ClockInstant {
    /// Converts an elapsed time in seconds (such as `GameClock::now()`) into a [`ClockInstant`].
    pub fn at(seconds: f64) -> Self {
        Self { epoch: (seconds / CLOCK_WRAP_PERIOD).floor() as u32, seconds: seconds.rem_euclid(CLOCK_WRAP_PERIOD) as f32 }
    }

    /// Current instant on Bevy's shader clock, matching `globals.time` and `shader_clock_epoch`.
    pub fn shader_now(time: &Time) -> Self {
        let epoch = time.elapsed().as_nanos() / time.wrap_period().as_nanos();
        Self { epoch: epoch as u32, seconds: time.elapsed_secs_wrapped() }
    }

    /// Elapsed seconds from `self` to `later`. Mirrors `clock_seconds_between` in
    /// `assets/shaders/common/clock.wesl`.
    pub fn seconds_until(self, later: Self) -> f32 {
        (later.epoch as f32 - self.epoch as f32) * CLOCK_WRAP_PERIOD as f32 + (later.seconds - self.seconds)
    }
}
