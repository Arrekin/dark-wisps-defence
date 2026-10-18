import bevy_sprite_render::mesh2d::vertex_output::VertexOutput;
import package::shaders::common::core::TAU;
import package::shaders::units::scan_pulse::scan_pulse;

// Scan spot shader - circle projection on ground
// Field order and types mirror `ScanSpotVisual` in units_internal/src/expedition_drone.rs.
struct Instance {
    seed: f32,  // scan pulse phase offset
};

const BASE_COLOR: vec3<f32> = vec3<f32>(0.3, 0.85, 1.0);
const SPOT_RING_WIDTH: f32 = 0.28;
const SPOT_RING_INTENSITY: f32 = 0.9;
const SPOT_GLOW_INTENSITY: f32 = 0.15;
const SPOT_SCAN_INTENSITY: f32 = 0.25;
const SPOT_CENTER_INTENSITY: f32 = 0.7;
const SPOT_MAX_ALPHA: f32 = 0.85;

fn look(mesh: VertexOutput, spot: Instance) -> vec4<f32> {
    let uv = mesh.uv;

    let center = vec2<f32>(0.5, 0.5);
    let dist = distance(uv, center) * 2.0; // 0 at center, 1 at edge

    // Thin outer ring where cone connects
    let ring_inner = 1.0 - SPOT_RING_WIDTH;
    let outer_ring = smoothstep(1.02, ring_inner + 0.02, dist) * smoothstep(ring_inner, ring_inner + 0.02, dist) * SPOT_RING_INTENSITY;

    // Soft inner glow
    let inner_glow = smoothstep(0.95, 0.0, dist) * SPOT_GLOW_INTENSITY;

    // Animated scan rings expanding outward
    let ring_phase = scan_pulse(spot.seed) * TAU;
    let rings = sin(dist * 10.0 - ring_phase) * 0.5 + 0.5;
    let ring_effect = rings * smoothstep(0.95, 0.2, dist) * SPOT_SCAN_INTENSITY;

    // Center bright spot
    let center_spot = smoothstep(0.2, 0.0, dist) * SPOT_CENTER_INTENSITY;

    let alpha = outer_ring + inner_glow + ring_effect + center_spot;
    return vec4<f32>(BASE_COLOR, clamp(alpha, 0.0, SPOT_MAX_ALPHA));
}
