// Game clock uploaded by session_internal through the shared GPU group.
// Custom materials enable this group through visuals::shared_gpu; lane shaders include it.

// Mirrors `GameClock::WRAP_PERIOD` in session/src/lib.rs.
const GAME_TIME_WRAP_PERIOD: f32 = 3600.0;

// The game clock in seconds, wrapped to `GAME_TIME_WRAP_PERIOD`. Stops while paused.
@group(3) @binding(101) var<storage, read> wrapped_game_time: f32;

// Wrapped game time. Use it as an animation phase, or with `game_time_since` for ages.
fn game_time() -> f32 {
    return wrapped_game_time;
}

// Elapsed seconds since a timestamp from `session::GameClock::wrapped`, accounting for one wrap.
// Elapsed time must be shorter than `GAME_TIME_WRAP_PERIOD`.
fn game_time_since(timestamp: f32) -> f32 {
    let age = wrapped_game_time - timestamp;
    return select(age, age + GAME_TIME_WRAP_PERIOD, age < 0.0);
}
