#import bevy_core_pipeline::fullscreen_vertex_shader::FullscreenVertexOutput
#import dwd::value_noise::{dwd_value_fbm_2d, dwd_value_noise_2d, dwd_value_noise_curl_2d}

@group(0) @binding(0) var screen_texture: texture_2d<f32>;
@group(0) @binding(1) var screen_sampler: sampler;
@group(0) @binding(2) var<uniform> camera: CameraData;
@group(0) @binding(3) var<storage, read> fields: array<ForceFieldEntry>;
@group(0) @binding(4) var<storage, read> ripples: array<FieldRipple>;

struct CameraData {
    world_pos: vec2<f32>,
    viewport_size: vec2<f32>,
    global_time: f32,
    field_count: u32,
}

struct ForceFieldEntry {
    center: vec2<f32>,
    radius: f32,
    progress: f32,
    visual_noise_offset: f32,
}

struct FieldRipple {
    hit_pos:      vec2<f32>,
    field_center: vec2<f32>,
    // spawn_time < 0 marks a padding entry (empty buffer sentinel).
    spawn_time:   f32,
    _pad:         f32,
}

// Tunables
const BASE_OPACITY:        f32 = 0.07;
const SEAM_OPACITY:        f32 = 0.62;
const SEAM_WIDTH:          f32 = 0.045;
const JUNCTION_ROUNDNESS:  f32 = 1.5;
const REFRACTION_STRENGTH: f32 = 4.0;
const NOISE_SCALE:         f32 = 0.0218;
const NOISE_SCROLL_SPEED:  f32 = 0.051;
// Each deeper layer evolves this many times faster than the one above it.
const LAYER_SPEED_MULT:    f32 = 1.5;
// Noise-space separation between the outermost and innermost depth slice at the rim.
const DEPTH_SCALE:            f32 = 1.6;
// Each outer layer samples noise at this much higher frequency than the one below it.
// Higher = crispier outer shell, softer inner glow. Mirrors LAYER_SPEED_MULT logic.
const LAYER_NOISE_SCALE_MULT: f32 = 5.4;

// Impact ripples
const RIPPLE_SPEED:       f32 = 60.0;  // wavefront expansion in world units / second
// Stretch factor along the rim→center axis. π/2 is the arc/chord ratio for a hemisphere:
// the wave travels π/2 times farther in 3D going over the dome than skirting the edge.
// Values > 1 squish the ring into an oblate ellipse, implying curvature upward.
const DOME_WARP:          f32 = 1.57;
// Fraction of eff_r at which the wave originates (< 1 = inside the rim).
// 1.0 = right at the edge; lower values push the start point up the dome.
const RIPPLE_INSET:       f32 = 0.91;
const RIPPLE_LIFETIME:    f32 = 3.0;   // seconds until a slot is ignored (matches CPU-side)
const RIPPLE_DECAY:       f32 = 0.7;   // exponential time-based envelope falloff
const RIPPLE_PULSE_WIDTH: f32 = 0.15;  // Gaussian width in phase² (world units²); larger = narrower ring
const RIPPLE_STRENGTH:    f32 = 6.0;   // peak refraction displacement magnitude
const RIPPLE_GLOW:        f32 = 0.5;   // peak additive brightness at the wavefront

// Centre spark
const SPARK_RADIUS:    f32 = 0.062;  // norm_dist at which the centre glow fades out
const SPARK_OPACITY:   f32 = 0.95;  // peak brightness of the centre point
// Ground contact ring
const GROUND_RING_OPACITY: f32 = 0.18;  // peak brightness of the contact rim at the edge

// ── Coordinate helpers ───────────────────────────────────────────────────────

fn uv_to_world(uv: vec2<f32>) -> vec2<f32> {
    let centered = uv - vec2<f32>(0.5, 0.5);
    return camera.world_pos + centered * camera.viewport_size * vec2<f32>(1.0, -1.0);
}

fn world_to_uv(world: vec2<f32>) -> vec2<f32> {
    let centered = (world - camera.world_pos) / camera.viewport_size;
    return centered * vec2<f32>(1.0, -1.0) + vec2<f32>(0.5, 0.5);
}

// ── Noise ────────────────────────────────────────────────────────────────────

// Crushes low noise values to zero and sharpens peaks into distinct wisps.
// Anything below the low threshold disappears; above high threshold → full bright.
fn wisp(raw: f32) -> f32 {
    return pow(smoothstep(0.40, 0.80, raw), 2.0);
}

// Ridged noise: bright sharp tendrils along the 0.5-iso-contours of the base
// noise field. Soft blobs become luminous filaments and wisps.
fn ridged_fbm(p: vec2<f32>) -> f32 {
    let n0 = 1.0 - abs(dwd_value_noise_2d(p)                              * 2.0 - 1.0);
    let n1 = 1.0 - abs(dwd_value_noise_2d(p * 2.1 + vec2<f32>(4.3, 1.7)) * 2.0 - 1.0);
    return n0 * 0.6 + n1 * 0.4;
}

// ── Fragment ─────────────────────────────────────────────────────────────────

@fragment
fn fragment(in: FullscreenVertexOutput) -> @location(0) vec4<f32> {
    let count = camera.field_count;
    if count == 0u {
        return textureSampleLevel(screen_texture, screen_sampler, in.uv, 0.0);
    }

    let world_pos = uv_to_world(in.uv);

    // ── Voronoi: find the three nearest fields by weighted distance ────────
    var best_wd      = 1.0e9;
    var second_wd    = 1.0e9;
    var third_wd     = 1.0e9;
    var best_idx     = 0u;
    var best_raw_d   = 0.0;

    for (var i = 0u; i < count; i++) {
        let raw_d      = distance(world_pos, fields[i].center);
        let eff_r      = fields[i].radius * fields[i].progress;
        if eff_r < 0.001 { continue; }
        let wd = raw_d / eff_r;
        if wd < best_wd {
            third_wd    = second_wd;
            second_wd   = best_wd;
            best_wd     = wd;
            best_idx    = i;
            best_raw_d  = raw_d;
        } else if wd < second_wd {
            third_wd    = second_wd;
            second_wd   = wd;
        } else if wd < third_wd {
            third_wd    = wd;
        }
    }

    // Outside all active fields entirely → pass through unchanged
    if best_wd > 1.0 {
        return textureSampleLevel(screen_texture, screen_sampler, in.uv, 0.0);
    }

    let field       = fields[best_idx];
    let eff_r       = field.radius * field.progress;
    // Normalised 0 (centre) → 1 (edge) within the winning field
    let norm_dist   = best_raw_d / eff_r;

    // ── Dome profile: physically-correct shell thickness ─────────────────────
    // At centre you look straight through thin shell → near zero opacity.
    // At the rim the line of sight is near-tangent → much thicker cross-section.
    // cos_theta = cos of the viewing angle through the sphere surface.
    // shell_thickness = 1/cos_theta - 1: 0 at centre, grows steeply at rim.
    let cos_theta = sqrt(max(0.001, 1.0 - norm_dist * norm_dist));
    let dome      = clamp((1.0 / cos_theta - 1.0) * 0.5, 0.0, 1.0)
                  * smoothstep(1.0, 0.82, norm_dist);

    // ── Per-field noise fluctuation (parallax depth layers) ──────────────────
    let to_pixel   = world_pos - field.center;
    let len_tp     = length(to_pixel);
    let radial_dir = select(vec2<f32>(1.0, 0.0), to_pixel / len_tp, len_tp > 0.1);

    let base_uv = to_pixel * NOISE_SCALE;
    let t  = (camera.global_time + field.visual_noise_offset) * NOISE_SCROLL_SPEED;
    let t1 = t * LAYER_SPEED_MULT;
    let t2 = t * LAYER_SPEED_MULT * LAYER_SPEED_MULT;

    // Curl noise flow fields: outer layers get the fastest flow (most turbulent
    // surface), inner layers are slowest (stable deep interior). This ensures
    // crisp outer tendrils roll dynamically over a slow-moving soft inner glow,
    // not the other way around.
    let warp_uv = base_uv * 0.5;
    let flow0 = dwd_value_noise_curl_2d(warp_uv + vec2<f32>(t2,        0.5));  // s0 outer  — fastest
    let flow1 = dwd_value_noise_curl_2d(warp_uv + vec2<f32>(t1 + 1.3,  2.1));  // s1 mid    — medium
    let flow2 = dwd_value_noise_curl_2d(warp_uv + vec2<f32>(t  + 2.7,  4.8));  // s2 inner  — slowest

    // Each layer samples at its own noise frequency: outer (s0) crispiest,
    // inner (s3) softest. Mirrors the LAYER_SPEED_MULT progression.
    let m2 = LAYER_NOISE_SCALE_MULT * LAYER_NOISE_SCALE_MULT;

    // Flow displacement is divided by each layer's scale multiplier.
    // At LAYER_NOISE_SCALE_MULT=1 nothing changes. At high values the outer layer's
    // warp is attenuated — preventing moiré in the crisp fine detail.
    // A sinusoidal oscillation keeps the outer layer visually alive without linear
    // drift (zero mean → no accumulation, no wave formation).
    // Inner layers still flow and drift at full strength.
    let osc_t     = camera.global_time * 0.13 + field.visual_noise_offset;
    let osc0      = vec2<f32>(sin(osc_t) * 0.35, cos(osc_t * 0.73) * 0.35);
    let noise_uv  = to_pixel * (NOISE_SCALE * m2)                     + flow0 * (1.2 / m2) + osc0;
    let noise_uv1 = to_pixel * (NOISE_SCALE * LAYER_NOISE_SCALE_MULT) + flow1 * (1.2 / LAYER_NOISE_SCALE_MULT) + vec2<f32>(3.7, 8.1);
    let noise_uv2 = to_pixel *  NOISE_SCALE                           + flow2 * 1.2 + vec2<f32>(7.3, 1.9);

    // Pseudo-volumetric: 4 depth slices through the shell cross-section.
    // depth_dir is radial and scales with norm_dist so at the rim (glancing angle)
    // the slices fan apart across a thick shell cross-section → real 3D depth.
    // At the centre norm_dist≈0 so all slices converge → thin transparent cap.
    let depth_dir = radial_dir * norm_dist * DEPTH_SCALE;

    // Each slice must start from an independent noise region — sharing a base
    // flow and adding only a radial depth offset produces shifted ring copies.
    // s0 → flow0 | s1 → flow1 | s2 → flow2 | s3 → flow0 + large seed jump
    // The depth_dir offset then adds parallax within each slice's own region
    // rather than duplicating another slice's pattern.
    let s0 = wisp(ridged_fbm(noise_uv));
    let s1 = wisp(ridged_fbm(noise_uv1                       + depth_dir * 0.33));
    let s2 = wisp(dwd_value_fbm_2d(noise_uv2                              + depth_dir * 0.67));
    let s3 = wisp(dwd_value_fbm_2d(to_pixel * (NOISE_SCALE / LAYER_NOISE_SCALE_MULT) + flow0 * (1.2 * LAYER_NOISE_SCALE_MULT) + vec2<f32>(23.1, 9.4) + depth_dir));
    let fluctuation = (s0 * 0.45 + s1 * 0.28 + s2 * 0.17 + s3 * 0.10) * 0.50;

    // ── Seam line where two bubbles press together ───────────────────────────
    var seam = 0.0;
    // Guard: second field must actually cover this point (second_wd <= 1.0).
    // Without this the seam bleeds past the physical contact surface into the
    // region where only one field exists.
    if second_wd <= 1.0 {
        // Pure geometric bisector — the Voronoi boundary is already a smooth
        // analytic curve. Warping it with the dome's high-freq noise made it jagged.
        let prox_12 = second_wd - best_wd;
        let seam_12 = smoothstep(SEAM_WIDTH, 0.0, prox_12)
                    * smoothstep(1.0, 0.82, second_wd);

        // At triple junctions draw the arm toward the third field and a rounded
        // Plateau-border blob where all three arms meet.
        var junction_fill = 0.0;
        if third_wd <= 1.0 {
            let prox_13 = third_wd - best_wd;
            let seam_13 = smoothstep(SEAM_WIDTH, 0.0, prox_13)
                        * smoothstep(1.0, 0.82, third_wd);
            // length() in (prox_12, prox_13) space is the true distance from the
            // triple junction. smoothstep on it produces a circular blob — unlike
            // the product seam_12*seam_13 which gives a rectangular overlap and
            // leaves sharp corners.
            let junction_dist = length(vec2<f32>(prox_12, prox_13));
            let junction_blob = smoothstep(SEAM_WIDTH * 2.0, 0.0, junction_dist) * JUNCTION_ROUNDNESS;
            junction_fill = seam_13 + junction_blob;
        }

        seam = clamp(seam_12 + junction_fill, 0.0, 1.0);
        // Slow large-scale brightness undulation along the film — energy
        // concentrations drifting like soap-bubble iridescence. Uses world_pos
        // at a much lower frequency than the dome noise so it stays smooth.
        let seam_coord = world_pos * (NOISE_SCALE * 0.5) + camera.global_time * 0.015;
        let seam_glow  = dwd_value_fbm_2d(seam_coord);
        seam *= 0.45 + seam_glow * 0.55;
    }
    // Extra brightness while a field is still growing (fight-boost)
    let fight_boost = 1.0 + (1.0 - field.progress) * 0.6;

    // ── Impact ripples: membrane displacement wave from wisp entry point ─────
    // The wavefront expands radially from hit_pos; the Gaussian envelope in phase
    // space produces a single traveling pulse rather than persistent ringing.
    var ripple_disp = vec2<f32>(0.0);
    var ripple_glow = 0.0;
    for (var ri = 0u; ri < arrayLength(&ripples); ri++) {
        let r = ripples[ri];
        if r.spawn_time < 0.0 { continue; }
        let age = camera.global_time - r.spawn_time;
        if age < 0.0 || age > RIPPLE_LIFETIME { continue; }
        // Skip slots belonging to a different dome (exact float match is fine — fields don't move).
        if distance(r.field_center, field.center) > 1.0 { continue; }

        // Place the wave origin at RIPPLE_INSET * eff_r from the center — slightly inside
        // the rim so the impact reads as hitting the dome above ground level, not at the base.
        let hit_to_center = normalize(field.center - r.hit_pos);
        let inset_r       = eff_r * RIPPLE_INSET;
        let rim_hit       = field.center - hit_to_center * inset_r;

        // Elliptical propagation: the wave slows down as it climbs over the dome top.
        // Decompose rim_hit→pixel into the rim-to-center axis and the perpendicular arm.
        // Stretching the along-axis by DOME_WARP (π/2) produces oblate iso-curves —
        // the ring spreads quickly along the rim and more slowly up and over the dome.
        let delta     = world_pos - rim_hit;
        let along     = dot(delta, hit_to_center);
        let across    = length(delta - along * hit_to_center);
        let dome_dist = length(vec2<f32>(along * DOME_WARP, across));

        // ForceFieldEntered fires on Voronoi cell entry, which can be outside the visual dome.
        // Subtract the time the wavefront would have spent travelling from hit_pos to the rim
        // so that expand_age=0 corresponds to the moment the wisp visually crosses the border.
        let hit_dist    = length(r.hit_pos - field.center);
        let pre_rim_t   = max(0.0, hit_dist - inset_r) / RIPPLE_SPEED;
        let expand_age  = max(0.0, age - pre_rim_t);
        let phase       = dome_dist - expand_age * RIPPLE_SPEED;

        let envelope  = exp(-age * RIPPLE_DECAY) * exp(-phase * phase * RIPPLE_PULSE_WIDTH);
        // cos peaks at the wavefront (phase=0) so the displacement ring sits exactly on the
        // expanding front rather than the zero-crossing that sin would produce there.
        let value       = cos(phase * 1.5) * envelope;
        let dist_to_hit = distance(world_pos, rim_hit);
        let hit_dir     = select(radial_dir, normalize(world_pos - rim_hit), dist_to_hit > 1.0);
        ripple_disp += hit_dir * value;
        // dome modulates glow so the ring appears brighter near the rim (more shell thickness).
        ripple_glow += max(0.0, value) * dome;
    }

    // ── Subtle radial refraction inside the dome ─────────────────────────────
    // ripple_disp is added on top and masked by field.progress so it respects growing fields.
    let refraction   = radial_dir * dome * REFRACTION_STRENGTH * (1.0 + fluctuation * 0.5)
                     + ripple_disp * RIPPLE_STRENGTH * field.progress;
    let displaced_uv = world_to_uv(world_pos - refraction);

    let base_color   = textureSampleLevel(screen_texture, screen_sampler, displaced_uv, 0.0);

    // ── Centre generation point ───────────────────────────────────────────────
    // Tight radial falloff gives a glowing source at the field origin.
    let spark_falloff = pow(1.0 - smoothstep(0.0, SPARK_RADIUS, norm_dist), 2.0);
    // High-frequency noise sampled at the fastest time scale → crackling shimmer.
    let spark_uv  = to_pixel * NOISE_SCALE * 25.0 + vec2<f32>(t2 * 3.5, t2 * 2.1);
    let shimmer   = dwd_value_fbm_2d(spark_uv);
    let spark     = spark_falloff * (0.35 + shimmer * 0.65) * field.progress;

    // ── Compose field colour ─────────────────────────────────────────────────
    // Cubic rim weight: wisps are almost invisible at centre, concentrated sharply at edge.
    let rim_weight  = pow(norm_dist, 3.0);
    let field_alpha = dome * (BASE_OPACITY * 0.3 + fluctuation * rim_weight * 1.1) * field.progress;
    let seam_alpha  = seam * SEAM_OPACITY * fight_boost;
    let final_alpha = max(field_alpha, seam_alpha);

    let dome_color = vec3<f32>(0.3, 0.7, 1.0);
    let seam_color = vec3<f32>(0.6, 0.9, 1.0);
    let tint = mix(dome_color, seam_color, seam * fight_boost);

    // ── Ground contact ring ───────────────────────────────────────────────────
    // Where the dome presses against the surface the viewing angle is most glancing,
    // so in reality this is the brightest, most energetic part of the shell.
    // A noise-modulated bright rim makes it read as physical rather than geometric.
    let ground_ring  = smoothstep(0.86, 0.97, norm_dist) * smoothstep(1.0, 0.91, norm_dist);
    let ground_noise = dwd_value_fbm_2d(noise_uv2 + vec2<f32>(5.1, 2.3));
    let ground_alpha = ground_ring * GROUND_RING_OPACITY * (0.44 + ground_noise * 0.56) * field.progress;
    let ground_color = vec3<f32>(0.45, 0.82, 1.0);

    // Centre glow added on top — bright near-white so it reads as a hot source.
    let spark_color  = vec3<f32>(0.75, 0.93, 1.0);
    let ripple_color = vec3<f32>(0.5, 0.9, 1.0);
    let result = base_color.rgb + tint * final_alpha + ground_color * ground_alpha
               + spark_color * (spark * SPARK_OPACITY)
               + ripple_color * ripple_glow * RIPPLE_GLOW * field.progress;
    return vec4<f32>(result, 1.0);
}
