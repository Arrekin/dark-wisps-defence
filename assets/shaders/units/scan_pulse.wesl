import package::shaders::common::game_time::game_time;

// The scan pulse: a 0..1 phase that loops on the game clock, so it stops while paused.

// Pulse loops per second.
const SCAN_PULSE_SPEED: f32 = 0.8;

fn scan_pulse(seed: f32) -> f32 {
    return fract(game_time() * SCAN_PULSE_SPEED + seed);
}
