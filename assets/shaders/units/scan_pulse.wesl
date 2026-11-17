import package::shaders::common::clock::{game_time, wrap_rate};

// The scan pulse: a 0..1 phase that loops on the game clock, so it stops while paused.

// Pulse loops per second.
const SCAN_PULSE_SPEED: f32 = 0.8;

fn scan_pulse(seed: f32) -> f32 {
    return fract(game_time() * wrap_rate(SCAN_PULSE_SPEED, 1.0) + seed);
}
