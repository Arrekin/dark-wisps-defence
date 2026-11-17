// Exponential fade shared by the shader-driven widgets. The Rust side writes `Fade` into each
// material uniform and samples the same curve on the CPU to restart a fade mid-flight; both live in
// `widgets/src/common/fade.rs` and must stay in step with this file.
//
// `Fade.start` is stamped on Bevy's shader clock; `shader_time_since` reads the shared GPU group.

import package::shaders::common::clock::{ClockInstant, shader_time_since};

// One state's fade. The rate travels with the fade so the curve is defined once, on the Rust side.
struct Fade {
    start_value: f32,
    end_value: f32,
    rate: f32,
    // Uniform buffers require nested structs to be 16-byte aligned.
    @align(16) start: ClockInstant,
};

// Current value of a fade. Callers pass `globals.time`.
fn eased(fade: Fade, now: f32) -> f32 {
    let elapsed = max(shader_time_since(fade.start, now), 0.0);
    return mix(fade.start_value, fade.end_value, 1.0 - exp(-fade.rate * elapsed));
}
