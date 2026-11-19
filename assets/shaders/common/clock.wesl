// Shader clocks: Bevy's `globals.time` and the game clock. Both wrap to zero every
// `CLOCK_WRAP_PERIOD` seconds to preserve `f32` precision over long sessions.
//
// `ClockInstant` pairs a clock's completed wrap count (`epoch`) with its wrapped `seconds`,
// so elapsed time between instants remains continuous across wraps. Reading either clock's
// current instant requires the shared GPU group (`visuals::shared_gpu`, included by lanes).
//
// Seamless animation across the wrap boundary:
// - Periodic motion: quantize periods (`wrap_period`) or rates (`wrap_rate`) so an integer
//   number of cycles fits into `CLOCK_WRAP_PERIOD`. Fold per-cycle indices with `wrap_index`.
// - Drifting noise: use `*_drift` in `common/noise.wesl` (folds `shader_clock_epoch` into the
//   texture tile period for any velocity) or in `common/value_noise.wesl` (quantizes velocity to
//   tile the lattice once per wrap).
// - Non-periodic hash noise (e.g., gradient or simplex noise scrolling across a plane): crossfade
//   into the wrap-shifted sample (`time - CLOCK_WRAP_PERIOD`) using `wrap_fade` and `wrap_mix`.

// Mirrors `visuals::clock::CLOCK_WRAP_PERIOD`.
const CLOCK_WRAP_PERIOD: f32 = 3600.0;
// Duration in seconds before `CLOCK_WRAP_PERIOD` over which `wrap_fade` ramps from 0 to 1.
const WRAP_FADE: f32 = 20.0;

// ── Instants ────────────────────────────────────────────────────────────────────────────

// Timestamp on a wrapped clock. Mirrors `visuals::clock::ClockInstant`.
struct ClockInstant {
    epoch: u32,
    seconds: f32,
};

// Elapsed seconds from `earlier` to `later`. Mirrors `ClockInstant::seconds_until` in
// visuals/src/clock.rs.
fn clock_seconds_between(earlier: ClockInstant, later: ClockInstant) -> f32 {
    return (f32(later.epoch) - f32(earlier.epoch)) * CLOCK_WRAP_PERIOD + (later.seconds - earlier.seconds);
}

// ── Bevy's shader clock ─────────────────────────────────────────────────────────────────

// Completed wrap periods of Bevy's shader clock (`globals.time` holds the wrapped seconds).
// Uploaded by visuals_internal.
@group(3) @binding(114) var<storage, read> shader_clock_epoch: u32;

// Current instant on Bevy's shader clock, given `now` (`globals.time`).
fn shader_clock_now(now: f32) -> ClockInstant {
    return ClockInstant(shader_clock_epoch, now);
}

// Elapsed seconds on Bevy's shader clock since `start` (stamped with `ClockInstant::shader_now`),
// given `now` (`globals.time`).
fn shader_time_since(start: ClockInstant, now: f32) -> f32 {
    return clock_seconds_between(start, shader_clock_now(now));
}

// ── Game clock ──────────────────────────────────────────────────────────────────────────

// Current game-clock instant. Pauses when the game is paused. Uploaded by session_internal.
@group(3) @binding(101) var<storage, read> game_clock: ClockInstant;

// Wrapped game time in `[0, CLOCK_WRAP_PERIOD)`. Use for continuous phase; use `game_time_since`
// for elapsed time.
fn game_time() -> f32 {
    return game_clock.seconds;
}

// Elapsed game-clock seconds since `start` (stamped with `ClockInstant::at`).
fn game_time_since(start: ClockInstant) -> f32 {
    return clock_seconds_between(start, game_clock);
}

// ── Seamless animation across the wrap ──────────────────────────────────────────────────

// Integer number of cycles (at least 1) of ~`period` seconds that fit into `CLOCK_WRAP_PERIOD`.
fn wrap_count(period: f32) -> f32 {
    return max(floor(CLOCK_WRAP_PERIOD / period + 0.5), 1.0);
}

// Period nearest to `period` that divides `CLOCK_WRAP_PERIOD` into an integer number of cycles.
fn wrap_period(period: f32) -> f32 {
    return CLOCK_WRAP_PERIOD / wrap_count(period);
}

// Rate nearest to `rate` that completes an integer number of `cycle`-sized periods per
// `CLOCK_WRAP_PERIOD`, preserving sign. Pass `cycle = 1.0` for unit/lattice phases or `TAU` for radians.
fn wrap_rate(rate: f32, cycle: f32) -> f32 {
    return sign(rate) * cycle / wrap_period(cycle / max(abs(rate), 1e-6));
}

// Folds an integer `index` into `0..round(count) - 1`. Rounding `count` and biasing by 0.5
// avoids float precision landing on `count`.
fn wrap_index(index: f32, count: f32) -> f32 {
    let whole = round(count);
    return index - whole * floor((index + 0.5) / whole);
}

// Crossfade weight in `[0, 1]` over the final `WRAP_FADE` seconds before `CLOCK_WRAP_PERIOD`.
fn wrap_fade(time: f32) -> f32 {
    return smoothstep(CLOCK_WRAP_PERIOD - WRAP_FADE, CLOCK_WRAP_PERIOD, time);
}

// Variance-preserving blend around 0.5 between a `[0, 1]` noise sample at `time` (`now`) and at
// `time - CLOCK_WRAP_PERIOD` (`wrapped`).
fn wrap_mix(now: f32, wrapped: f32, fade: f32) -> f32 {
    return 0.5 + (mix(now, wrapped, fade) - 0.5) / sqrt(fade * fade + (1.0 - fade) * (1.0 - fade));
}
