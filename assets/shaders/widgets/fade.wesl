
// Exponential fade shared by the shader-driven widgets. The Rust side writes `Fade` into each
// material uniform and samples the same curve on the CPU to restart a fade mid-flight; both live in
// `widgets/src/common/fade.rs` and must stay in step with this file.

// One state's fade. The rate travels with the fade so the curve is defined once, on the Rust side.
struct Fade {
    start_value: f32,
    end_value: f32,
    start_time: f32,
    rate: f32,
};

// Current value of a fade. Callers pass `globals.time` for framerate-independent animation.
fn eased(fade: Fade, now: f32) -> f32 {
    let elapsed = max(0.0, now - fade.start_time);
    return mix(fade.start_value, fade.end_value, 1.0 - exp(-fade.rate * elapsed));
}
