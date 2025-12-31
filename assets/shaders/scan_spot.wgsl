#import bevy_sprite::mesh2d_vertex_output::VertexOutput

// Scan spot shader - circle projection on ground
struct ScanSpotData {
    pulse: f32,  // animation 0-1
};

@group(2) @binding(0)
var<uniform> uniforms: ScanSpotData;

const BASE_COLOR: vec3<f32> = vec3<f32>(0.3, 0.85, 1.0);
const SPOT_RING_WIDTH: f32 = 0.28;
const SPOT_RING_INTENSITY: f32 = 0.9;
const SPOT_GLOW_INTENSITY: f32 = 0.15;
const SPOT_SCAN_INTENSITY: f32 = 0.25;
const SPOT_CENTER_INTENSITY: f32 = 0.7;
const SPOT_MAX_ALPHA: f32 = 0.85;

@fragment
fn fragment(mesh: VertexOutput) -> @location(0) vec4<f32> {
    let uv = mesh.uv;
    
    let center = vec2<f32>(0.5, 0.5);
    let dist = distance(uv, center) * 2.0; // 0 at center, 1 at edge
    
    // Thin outer ring where cone connects
    let ring_inner = 1.0 - SPOT_RING_WIDTH;
    let outer_ring = smoothstep(1.02, ring_inner + 0.02, dist) * smoothstep(ring_inner, ring_inner + 0.02, dist) * SPOT_RING_INTENSITY;
    
    // Soft inner glow
    let inner_glow = smoothstep(0.95, 0.0, dist) * SPOT_GLOW_INTENSITY;
    
    // Animated scan rings expanding outward
    let ring_phase = uniforms.pulse * 6.28318;
    let rings = sin(dist * 10.0 - ring_phase) * 0.5 + 0.5;
    let ring_effect = rings * smoothstep(0.95, 0.2, dist) * SPOT_SCAN_INTENSITY;
    
    // Center bright spot
    let center_spot = smoothstep(0.2, 0.0, dist) * SPOT_CENTER_INTENSITY;
    
    let alpha = outer_ring + inner_glow + ring_effect + center_spot;
    return vec4<f32>(BASE_COLOR, clamp(alpha, 0.0, SPOT_MAX_ALPHA));
}
