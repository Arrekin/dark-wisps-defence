// Shared dot distribution and arc geometry for lob preview body and shadow shaders.
//
// Dots are spaced at `1 / gaps` increments of arc progress (`~DOT_SPACING` world units on the
// ground, capped at `MAX_DOTS`). Keeping `gaps` continuous makes existing dots stretch smoothly as
// the arc lengthens while new dots emerge from the landing end (`progress = 1`), rather than
// snapping all dots when the count changes. Dots fade in and out over `END_FADE` at both ends.

const DOT_SPACING: f32 = 12.0;  // target ground spacing between dots, in world units
const MAX_DOTS: u32 = 48;
const END_FADE: f32 = 0.04;     // arc progress fraction over which dots fade in at launch and out at landing

// Continuous number of `DOT_SPACING` intervals between `origin` and `landing`, clamped to `[1, MAX_DOTS - 1]`.
fn dot_gaps(origin: vec2<f32>, landing: vec2<f32>) -> f32 {
    return clamp(distance(origin, landing) / DOT_SPACING, 1.0, f32(MAX_DOTS - 1u));
}

// Index of the final dot (clamped to `progress = 1.0` at landing).
fn last_dot(gaps: f32) -> u32 {
    return u32(ceil(gaps));
}

// Normalized arc progress in `[0, 1]` for dot `index`.
fn dot_progress(index: u32, gaps: f32) -> f32 {
    return min(f32(index) / gaps, 1.0);
}

// Endpoint fade factor in `[0, 1]` at `progress`, ramping over `END_FADE` from launch and landing.
fn dot_presence(progress: f32) -> f32 {
    return clamp(min(progress, 1.0 - progress) / END_FADE, 0.0, 1.0);
}

// Parabolic height above ground at `progress` (`[0, 1]`) for peak height `apex`.
fn arc_height(apex: f32, progress: f32) -> f32 {
    return 4.0 * apex * progress * (1.0 - progress);
}
