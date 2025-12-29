#import bevy_sprite::mesh2d_vertex_output::VertexOutput

// Dual-purpose shader:
// is_spot = 0.0: BEAM mode (draws cone from drone to spot)
// is_spot = 1.0: SPOT mode (draws circle with scan rings)
struct ScanningRayData {
    start_width: f32,  // beam: normalized width at drone end (small)
    end_width: f32,    // beam: normalized width at spot end (1.0 = full)
    pulse: f32,        // animation 0-1
    is_spot: f32,      // 0.0 = beam, 1.0 = spot
};

@group(2) @binding(0)
var<uniform> uniforms: ScanningRayData;

@fragment
fn fragment(mesh: VertexOutput) -> @location(0) vec4<f32> {
    let uv = mesh.uv;
    let base_color = vec3<f32>(0.3, 0.85, 1.0);
    
    // === SPOT MODE: Circle scan area on ground ===
    if (uniforms.is_spot > 0.5) {
        // Tunable spot constants
        let SPOT_RING_WIDTH = 0.28;      // thickness of outer ring (lower = thinner)
        let SPOT_RING_INTENSITY = 0.9;   // brightness of outer ring
        let SPOT_GLOW_INTENSITY = 0.15;  // inner glow brightness
        let SPOT_SCAN_INTENSITY = 0.25;  // animated rings brightness
        let SPOT_CENTER_INTENSITY = 0.7; // center spot brightness
        
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
        return vec4<f32>(base_color, clamp(alpha, 0.0, 0.85));
    }
    
    // === BEAM MODE: Cone from drone apex to spot ===
    // Tunable beam constants
    let BEAM_EDGE_INTENSITY = 0.9;    // edge line brightness
    let BEAM_EDGE_THICKNESS = 0.03;   // base edge line thickness
    let BEAM_FILL_INTENSITY = 0.15;    // interior fill brightness
    let BEAM_PULSE_INTENSITY = 0.65;  // traveling pulse brightness
    let BEAM_APEX_INTENSITY = 0.96;    // apex glow brightness
    let BEAM_MAX_ALPHA = 0.9;         // maximum overall alpha
    
    let x = uv.x;           // 0 = drone (apex), 1 = spot (base)
    let y = uv.y - 0.5;     // -0.5 to 0.5, 0 = center
    
    // Cone width: linear interpolation from narrow apex to wide base
    let half_width = mix(uniforms.start_width, uniforms.end_width, x) * 0.5;
    
    // Outside cone = transparent
    if (abs(y) > half_width) {
        return vec4<f32>(0.0, 0.0, 0.0, 0.0);
    }
    
    var alpha = 0.0;
    
    // === Cone edge lines (the "walls" of the cone) ===
    let dist_to_edge = abs(abs(y) - half_width);
    let edge_thickness = BEAM_EDGE_THICKNESS + 0.025 * x; // thicker toward spot
    let edge_line = smoothstep(edge_thickness, 0.0, dist_to_edge) * BEAM_EDGE_INTENSITY;
    
    // === Soft interior fill (light passing through cone) ===
    let center_dist = abs(y) / max(half_width, 0.001);
    let interior = (1.0 - center_dist * center_dist) * BEAM_FILL_INTENSITY * (0.7 + 0.3 * x);
    
    // === Animated scan pulse traveling down the beam ===
    let pulse_pos = uniforms.pulse;
    let pulse_width = 0.18;
    let pulse_strength = smoothstep(pulse_width, 0.0, abs(x - pulse_pos));
    let pulse_effect = pulse_strength * BEAM_PULSE_INTENSITY * (1.0 - center_dist * 0.4);
    
    // === Apex glow (bright point at drone) ===
    let apex_dist = length(vec2<f32>(x * 3.0, y));
    let apex_glow = smoothstep(0.25, 0.0, apex_dist) * BEAM_APEX_INTENSITY;
    
    // === Fade near spot end (beam merges into spot) ===
    let end_fade = smoothstep(1.0, 0.8, x);
    
    alpha = (edge_line + interior + pulse_effect) * end_fade + apex_glow;
    
    return vec4<f32>(base_color, clamp(alpha, 0.0, BEAM_MAX_ALPHA));
}
