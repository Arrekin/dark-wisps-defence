// Expanding ripple that pushes the scene outward and brightens the wave's leading edge.
// The quad spans the ripple's full diameter.

import bevy_sprite_render::mesh2d::vertex_output::VertexOutput;
import package::shaders::common::core::PI;
import package::shaders::common::displacement::world_offset_to_uv;

// Field order and types mirror `RippleDisplacement` in weaponry_internal/src/ripple.rs.
struct Instance {
    normalized_radius: f32,
    max_radius: f32,
};

// Wave shape and intensity
const WAVE_WIDTH: f32       = 0.15;
const BOOST_INTENSITY: f32  = 0.22;
const MAX_DISPLACEMENT: f32 = 15.0;
const LEADING_BRIGHTNESS: f32 = 1.2;

fn displace(in: VertexOutput, ripple: Instance) -> vec4<f32> {
    // Offset from the center in units of the full diameter, with Y increasing upward.
    let to_pixel = (in.uv - vec2<f32>(0.5)) * vec2<f32>(1.0, -1.0);
    let dist = length(to_pixel);

    let envelope = sin(ripple.normalized_radius * 2.0 * PI);
    let wave_width = max(WAVE_WIDTH * envelope, 0.001);
    if dist < (ripple.normalized_radius - wave_width) || dist > ripple.normalized_radius {
        return vec4<f32>(0.0);
    }

    let band_t = (dist - (ripple.normalized_radius - wave_width)) / wave_width;
    let wave_shape = sin(band_t * PI);
    let amplitude = wave_shape * MAX_DISPLACEMENT * envelope;
    // Keep the center's direction zero without dividing by zero.
    let radial_dir = to_pixel / max(dist, 1e-6);

    // The scene is sampled from closer to the centre, so the wave pushes it outward.
    let offset = world_offset_to_uv(-radial_dir * amplitude);

    let leading_t = max(0.0, band_t * 2.0 - 1.0);
    let gain = LEADING_BRIGHTNESS * leading_t * leading_t * envelope + BOOST_INTENSITY * wave_shape * envelope;

    return vec4<f32>(offset, gain, 0.0);
}
