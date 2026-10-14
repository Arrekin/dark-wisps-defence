// Approximate sRGB (gamma 2.2) <-> linear light conversions.

// sRGB display color -> linear light. Use on final composited colors tuned in sRGB space.
fn display_to_linear(color: vec3<f32>) -> vec3<f32> {
    return pow(max(color, vec3<f32>(0.0)), vec3<f32>(2.2));
}

// Linear light -> sRGB display color. Use when comparing sampled linear texels against sRGB thresholds.
fn linear_to_display(color: vec3<f32>) -> vec3<f32> {
    return pow(max(color, vec3<f32>(0.0)), vec3<f32>(1.0 / 2.2));
}
