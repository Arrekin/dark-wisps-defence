#import bevy_sprite::mesh2d_vertex_output::VertexOutput

// Scanning beam shader - cone from drone to spot
struct ScanningBeamData {
    start_width: f32,  // normalized width at drone end (narrow)
    end_width: f32,    // normalized width at spot end (wide)
    pulse: f32,        // animation 0-1
};

@group(2) @binding(0)
var<uniform> uniforms: ScanningBeamData;

const BASE_COLOR: vec3<f32> = vec3<f32>(0.3, 0.85, 1.0);
const BEAM_EDGE_INTENSITY: f32 = 0.9;
const BEAM_EDGE_THICKNESS: f32 = 0.03;
const BEAM_FILL_INTENSITY: f32 = 0.15;
const BEAM_PULSE_INTENSITY: f32 = 0.65;
const BEAM_APEX_INTENSITY: f32 = 0.96;
const BEAM_MAX_ALPHA: f32 = 0.9;

@fragment
fn fragment(mesh: VertexOutput) -> @location(0) vec4<f32> {
    let uv = mesh.uv;
    
    let x = uv.x;           // 0 = drone (apex), 1 = spot (base)
    let y = uv.y - 0.5;     // -0.5 to 0.5, 0 = center
    
    // Cone width: linear interpolation from narrow apex to wide base
    let half_width = mix(uniforms.start_width, uniforms.end_width, x) * 0.5;
    
    // Outside cone = transparent
    if (abs(y) > half_width) {
        return vec4<f32>(0.0, 0.0, 0.0, 0.0);
    }
    
    var alpha = 0.0;
    
    // Cone edge lines (the "walls" of the cone)
    let dist_to_edge = abs(abs(y) - half_width);
    let edge_thickness = BEAM_EDGE_THICKNESS + 0.025 * x;
    let edge_line = smoothstep(edge_thickness, 0.0, dist_to_edge) * BEAM_EDGE_INTENSITY;
    
    // Soft interior fill
    let center_dist = abs(y) / max(half_width, 0.001);
    let interior = (1.0 - center_dist * center_dist) * BEAM_FILL_INTENSITY * (0.7 + 0.3 * x);
    
    // Animated scan pulse traveling down the beam
    let pulse_pos = uniforms.pulse;
    let pulse_width = 0.18;
    let pulse_strength = smoothstep(pulse_width, 0.0, abs(x - pulse_pos));
    let pulse_effect = pulse_strength * BEAM_PULSE_INTENSITY * (1.0 - center_dist * 0.4);
    
    // Apex glow (bright point at drone)
    let apex_dist = length(vec2<f32>(x * 3.0, y));
    let apex_glow = smoothstep(0.25, 0.0, apex_dist) * BEAM_APEX_INTENSITY;
    
    // Fade near spot end (beam merges into spot)
    let end_fade = smoothstep(1.0, 0.8, x);
    
    alpha = (edge_line + interior + pulse_effect) * end_fade + apex_glow;
    
    return vec4<f32>(BASE_COLOR, clamp(alpha, 0.0, BEAM_MAX_ALPHA));
}
