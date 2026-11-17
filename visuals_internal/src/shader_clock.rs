//! Configures Bevy's virtual clock wrap period to [`CLOCK_WRAP_PERIOD`] and uploads its wrap
//! epoch for `assets/shaders/common/clock.wesl`.

use std::time::Duration;

use bevy::prelude::*;

use visuals::{clock::{CLOCK_WRAP_PERIOD, ClockInstant}, shared_gpu::{SharedGpuBinding, SharedGpuWriter}};

pub(crate) struct ShaderClockPlugin;
impl Plugin for ShaderClockPlugin {
    fn build(&self, app: &mut App) {
        app
            .add_systems(Startup, apply_clock_wrap_period)
            .add_systems(PostUpdate, upload_shader_clock_epoch);
    }
}

/// Sets the virtual clock's wrap period so `globals.time` wraps at [`CLOCK_WRAP_PERIOD`].
fn apply_clock_wrap_period(mut virtual_time: ResMut<Time<Virtual>>) {
    virtual_time.set_wrap_period(Duration::from_secs_f64(CLOCK_WRAP_PERIOD));
}

/// Uploads the current wrap epoch when it advances. Epoch 0 is covered by the buffer's zeroed placeholder.
fn upload_shader_clock_epoch(
    mut shared_gpu: SharedGpuWriter,
    time: Res<Time>,
    mut uploaded_epoch: Local<u32>,
) {
    let epoch = ClockInstant::shader_now(&time).epoch;
    if epoch == *uploaded_epoch { return }
    shared_gpu.write(SharedGpuBinding::ShaderClockEpoch, [epoch]);
    *uploaded_epoch = epoch;
}
