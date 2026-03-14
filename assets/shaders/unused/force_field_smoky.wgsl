#import bevy_core_pipeline::fullscreen_vertex_shader::FullscreenVertexOutput

@group(0) @binding(0) var screen_texture: texture_2d<f32>;
@group(0) @binding(1) var screen_sampler: sampler;
@group(0) @binding(2) var<uniform> camera: CameraData;
@group(0) @binding(3) var<storage, read> fields: array<ForceFieldEntry>;

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

// Tunables
const BASE_OPACITY:        f32 = 0.07;
const EDGE_OPACITY:        f32 = 0.18;
const SEAM_OPACITY:        f32 = 0.62;
const SEAM_WIDTH:          f32 = 0.045;
const JUNCTION_ROUNDNESS:  f32 = 1.5;
const REFRACTION_STRENGTH: f32 = 4.0;
const NOISE_SCALE:         f32 = 0.0018;
const NOISE_SCROLL_SPEED:  f32 = 0.051;
// Each deeper layer evolves this many times faster than the one above it.
const LAYER_SPEED_MULT:    f32 = 1.5;
// Noise-space separation between the outermost and innermost depth slice at the rim.
const DEPTH_SCALE:            f32 = 1.6;
// Each outer layer samples noise at this much higher frequency than the one below it.
// Higher = crispier outer shell, softer inner glow. Mirrors LAYER_SPEED_MULT logic.
const LAYER_NOISE_SCALE_MULT: f32 = 2.4;

// Centre generation point
const SPARK_RADIUS:    f32 = 0.062;  // norm_dist at which the centre glow fades out
const SPARK_OPACITY:   f32 = 0.95;  // peak brightness of the centre point
// Outward emanation: patterns stream from centre toward rim (world units / second).
// Set to 0.0 to disable.
const EMANATION_SPEED: f32 = 3.0;

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

fn hash(p: vec2<f32>) -> f32 {
    let h = dot(p, vec2<f32>(127.1, 311.7));
    return fract(sin(h) * 43758.5453123);
}

fn noise(p: vec2<f32>) -> f32 {
    let i = floor(p);
    let f = fract(p);
    let a = hash(i);
    let b = hash(i + vec2<f32>(1.0, 0.0));
    let c = hash(i + vec2<f32>(0.0, 1.0));
    let d = hash(i + vec2<f32>(1.0, 1.0));
    let u = f * f * (3.0 - 2.0 * f);
    return mix(mix(a, b, u.x), mix(c, d, u.x), u.y);
}

// Two-octave noise for richer fluctuation
fn fbm(p: vec2<f32>) -> f32 {
    return noise(p) * 0.6 + noise(p * 2.1 + vec2<f32>(4.3, 1.7)) * 0.4;
}

// Crushes low noise values to zero and sharpens peaks into distinct wisps.
// Anything below the low threshold disappears; above high threshold → full bright.
fn wisp(raw: f32) -> f32 {
    return pow(smoothstep(0.40, 0.80, raw), 2.0);
}

// Divergence-free 2D curl of a noise potential field.
// The result is a flow velocity in ~[-1, 1] that has no sources or sinks —
// parcels of cloud roll along streamlines instead of randomly drifting.
fn curl(p: vec2<f32>) -> vec2<f32> {
    let e = 0.5;
    return vec2<f32>(
         noise(p + vec2<f32>(0.0,  e)) - noise(p - vec2<f32>(0.0,  e)),
        -(noise(p + vec2<f32>(e, 0.0)) - noise(p - vec2<f32>(e, 0.0)))
    );
}

// Ridged noise: bright sharp tendrils along the 0.5-iso-contours of the base
// noise field. Soft blobs become luminous filaments and wisps.
fn ridged_fbm(p: vec2<f32>) -> f32 {
    let n0 = 1.0 - abs(noise(p)                              * 2.0 - 1.0);
    let n1 = 1.0 - abs(noise(p * 2.1 + vec2<f32>(4.3, 1.7)) * 2.0 - 1.0);
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
    let flow0 = curl(warp_uv + vec2<f32>(t2,        0.5));  // s0 outer  — fastest
    let flow1 = curl(warp_uv + vec2<f32>(t1 + 1.3,  2.1));  // s1 mid    — medium
    let flow2 = curl(warp_uv + vec2<f32>(t  + 2.7,  4.8));  // s2 inner  — slowest

    // Emanation: shift sample coords inward so noise features appear to stream
    // outward from the centre. Same world-space speed for all layers.
    let em = radial_dir * camera.global_time * EMANATION_SPEED;

    // Each layer samples at its own noise frequency: outer (s0) crispiest,
    // inner (s3) softest. Mirrors the LAYER_SPEED_MULT progression.
    let m2 = LAYER_NOISE_SCALE_MULT * LAYER_NOISE_SCALE_MULT;
    let noise_uv  = (to_pixel - em) * (NOISE_SCALE * m2)                      + flow0 * 1.2;
    let noise_uv1 = (to_pixel - em) * (NOISE_SCALE * LAYER_NOISE_SCALE_MULT)  + flow1 * 1.2 + vec2<f32>(3.7, 8.1);
    let noise_uv2 = (to_pixel - em) *  NOISE_SCALE                            + flow2 * 1.2 + vec2<f32>(7.3, 1.9);

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
    let s2 = wisp(fbm(noise_uv2                              + depth_dir * 0.67));
    let s3 = wisp(fbm((to_pixel - em) * (NOISE_SCALE / LAYER_NOISE_SCALE_MULT) + flow0 * 1.2 + vec2<f32>(23.1, 9.4) + depth_dir));
    let fluctuation = (s0 * 0.45 + s1 * 0.28 + s2 * 0.17 + s3 * 0.10) * 0.50;

    // ── Seam line where two bubbles press together ───────────────────────────
    var seam = 0.0;
    // Guard: second field must actually cover this point (second_wd <= 1.0).
    // Without this the seam bleeds past the physical contact surface into the
    // region where only one field exists.
    if second_wd <= 1.0 {
        // Warp the seam line with noise so it undulates with the dome surface
        // instead of being a static geometric bisector.
        let seam_noise = fbm(noise_uv + vec2<f32>(13.7, 5.3));
        let noise_warp = (seam_noise - 0.5) * 0.04;

        let prox_12 = (second_wd - best_wd) + noise_warp;
        let seam_12 = smoothstep(SEAM_WIDTH, 0.0, prox_12)
                    * smoothstep(1.0, 0.82, second_wd);

        // At triple junctions (third field also inside) both seam lines meet at
        // a sharp angle. Fill the corner with a Plateau-border blob: the product
        // seam_12 * seam_13 is non-zero only where both lines overlap, so adding
        // it rounds the junction into a smooth rounded node.
        var junction_fill = 0.0;
        if third_wd <= 1.0 {
            let prox_13 = (third_wd - best_wd) + noise_warp;
            let seam_13 = smoothstep(SEAM_WIDTH, 0.0, prox_13)
                        * smoothstep(1.0, 0.82, third_wd);
            junction_fill = seam_12 * seam_13 * JUNCTION_ROUNDNESS;
        }

        seam = clamp(seam_12 + junction_fill, 0.0, 1.0);
        // Modulate brightness with the same noise rhythm as the dome.
        seam *= 0.5 + seam_noise * 0.5;
    }
    // Extra brightness while a field is still growing (fight-boost)
    let fight_boost = 1.0 + (1.0 - field.progress) * 0.6;

    // ── Subtle radial refraction inside the dome ─────────────────────────────
    let refraction   = radial_dir * dome * REFRACTION_STRENGTH * (1.0 + fluctuation * 0.5);
    let displaced_uv = world_to_uv(world_pos - refraction);

    let base_color   = textureSampleLevel(screen_texture, screen_sampler, displaced_uv, 0.0);

    // ── Centre generation point ───────────────────────────────────────────────
    // Tight radial falloff gives a glowing source at the field origin.
    let spark_falloff = pow(1.0 - smoothstep(0.0, SPARK_RADIUS, norm_dist), 2.0);
    // High-frequency noise sampled at the fastest time scale → crackling shimmer.
    let spark_uv  = to_pixel * NOISE_SCALE * 25.0 + vec2<f32>(t2 * 3.5, t2 * 2.1);
    let shimmer   = fbm(spark_uv);
    let spark     = spark_falloff * (0.35 + shimmer * 0.65) * field.progress;

    // ── Compose field colour ─────────────────────────────────────────────────
    // Wisps concentrated toward the rim — reinforces the hollow-centre look.
    let rim_weight  = pow(norm_dist, 1.5);
    let field_alpha = dome * (BASE_OPACITY * 0.3 + fluctuation * rim_weight * 1.1) * field.progress;
    let seam_alpha  = seam * SEAM_OPACITY * fight_boost;
    let final_alpha = max(field_alpha, seam_alpha);

    let dome_color = vec3<f32>(0.3, 0.7, 1.0);
    let seam_color = vec3<f32>(0.6, 0.9, 1.0);
    let tint = mix(dome_color, seam_color, seam * fight_boost);

    // Centre glow added on top — bright near-white so it reads as a hot source.
    let spark_color = vec3<f32>(0.75, 0.93, 1.0);
    let result = base_color.rgb + tint * final_alpha + spark_color * (spark * SPARK_OPACITY);
    return vec4<f32>(result, 1.0);
}
