import bevy_sprite_render::mesh2d::vertex_output::VertexOutput;
import bevy_sprite_render::mesh2d::view_bindings::globals;
import package::shaders::common::core::TAU;
import package::shaders::common::fire_ramp::fire_ramp;
import package::shaders::common::gradient_noise::{gradient_fbm_2d, gradient_noise_2d};
import package::shaders::common::hash::{HASH_GOLDEN, hash_mix, hash_unit};

// Fireball impact with two layers sharing a square mesh centered on the landing cell:
// - Blast: flash, expanding fire and smoke, an outward-moving fire ring, splatter, and sparks.
//   Drawn above wisps.
// - Scorch: a fading burn mark with cooling cinders. Drawn below wisps.
//
// The raised-cell mask excludes ground effects (scorch, cinders, splatter, and ring) from walls,
// buildings, and dark ore. Airborne effects and light are not masked.
// Distances are in cells from the landing cell's center. Premultiplied output combines additive
// light with alpha-blended fire and smoke.

// Field order and types mirror `FireballImpactUniform` in weaponry_internal/src/fireball.rs.
struct FireballImpact {
    age: f32,             // seconds since impact
    seed: f32,
    layer: u32,           // 0 = blast, 1 = scorch
    duration: f32,        // seconds the layer lives
    reach: u32,           // mesh and mask extent in cells beyond the landing cell
    main_half_size: f32,  // main blast half-size, in cells
    ring_half_size: f32,  // outer ring half-size, in cells
    raised_low: u32,      // raised cells around the landing cell, one bit each, row by row
    raised_high: u32,
};

@group(2) @binding(0)
var<uniform> impact: FireballImpact;

const LAYER_SCORCH: u32 = 1u;

// ── Main blast ───────────────────────────────────────────────────────────────
const CORE_FILL: f32 = 1.15;      // fireball radius, relative to the main blast half-size
const CORE_GROW: f32 = 18.0;
const CORE_LIFE: f32 = 0.7;       // seconds
const SQUARENESS: f32 = 3.0;      // 2 = round, higher fills the square footprint
const NOISE_SCALE: f32 = 2.2;
const BILLOW_SPEED: f32 = 1.5;
const BIAS: f32 = 1.0;
const BURN: f32 = 0.9;            // how fast the fireball breaks into clumps
const THRESHOLD: f32 = 0.5;
const HEAT_START: f32 = 0.98;
const HEAT_END: f32 = 0.55;
const SMOKE: f32 = 0.42;
const FLASH: f32 = 1.2;
const FLASH_DECAY: f32 = 14.0;
const GROUND_LIGHT: f32 = 0.3;
const GROUND_LIGHT_FALLOFF: f32 = 2.5; // exponential decay coefficient for (distance / ring_half_size)^2

// ── Ring ─────────────────────────────────────────────────────────────────────
const RING_TIME: f32 = 0.35;      // seconds to reach the ring's edge
const RING_THICKNESS: f32 = 0.6;  // in cells
const RING_HEAT: f32 = 0.6;
const RING_LICKS: f32 = 2.5;      // angular noise scale of the ring's front
const SPLAT: f32 = 0.72;
const SPLAT_LIFE: f32 = 1.0;      // seconds
const SPLAT_SCALE: f32 = 1.6;
const SPARK_COUNT: u32 = 24u;
const SPARK_REACH: f32 = 1.2;     // relative to the ring half-size
const SPARK_LIFE: f32 = 0.6;      // seconds

// ── Scorch ───────────────────────────────────────────────────────────────────
const SCORCH: f32 = 0.6;
const SCORCH_SIZE: f32 = 1.1;     // relative to the main blast half-size
const CINDER_DECAY: f32 = 1.2;

const LIGHT_COLOR: vec3<f32> = vec3<f32>(1.0, 0.45, 0.15); // warm orange

// Superellipse norm: p = 2 is a circle, large p approaches a square.
fn shape_norm(q: vec2<f32>, p: f32) -> f32 {
    let a = abs(q) + vec2<f32>(1e-5);
    return pow(pow(a.x, p) + pow(a.y, p), 1.0 / p);
}

fn premultiplied_over(below: vec4<f32>, color: vec3<f32>, alpha: f32) -> vec4<f32> {
    return vec4<f32>(color * alpha + below.rgb * (1.0 - alpha), alpha + below.a * (1.0 - alpha));
}

fn is_raised_cell(cell: vec2<i32>) -> f32 {
    let reach = i32(impact.reach);
    let local = cell + vec2<i32>(reach);
    let size = 2 * reach + 1;
    if any(local < vec2<i32>(0)) || any(local >= vec2<i32>(size)) { return 1.0; }
    let index = u32(local.y * size + local.x);
    if index < 32u {
        return f32((impact.raised_low >> index) & 1u);
    }
    return f32((impact.raised_high >> (index - 32u)) & 1u);
}

// Raised-cell coverage: 1 on obstacles, 0 on open ground, with softened edges and rounded corners.
fn raised(q: vec2<f32>) -> f32 {
    let base = floor(q);
    let f = q - base;
    let cell = vec2<i32>(base);
    let bottom = mix(is_raised_cell(cell), is_raised_cell(cell + vec2<i32>(1, 0)), f.x);
    let top = mix(is_raised_cell(cell + vec2<i32>(0, 1)), is_raised_cell(cell + vec2<i32>(1, 1)), f.x);
    return smoothstep(0.3, 0.7, mix(bottom, top, f.y));
}

fn scorch_layer(q: vec2<f32>, age: f32, ground: f32) -> vec4<f32> {
    let main_blast = shape_norm(q, SQUARENESS) / (impact.main_half_size * SCORCH_SIZE);
    let edge_noise = gradient_fbm_2d(q * 2.0 + vec2<f32>(13.0, 3.0), 3);
    let scorch = (1.0 - smoothstep(0.7, 1.0, main_blast + (edge_noise - 0.5) * 0.8))
        * SCORCH * smoothstep(0.0, 0.2, age) * (1.0 - smoothstep(impact.duration * 0.4, impact.duration, age)) * ground;
    var out = premultiplied_over(vec4<f32>(0.0), vec3<f32>(0.0), scorch * 0.75);
    let cinders = smoothstep(0.66, 0.72, gradient_fbm_2d(q * 5.0 + vec2<f32>(7.0, 2.0), 3)) * scorch * exp(-age * CINDER_DECAY);
    out = vec4<f32>(out.rgb + fire_ramp(0.6) * cinders * 1.5, out.a);
    return out;
}

fn blast_layer(q: vec2<f32>, age: f32, time: f32, ground: f32) -> vec4<f32> {
    let main_half = impact.main_half_size;
    let ring_half = impact.ring_half_size;
    let m = shape_norm(q, SQUARENESS);
    let dist = length(q);
    let dir = q / max(dist, 1e-3);
    var out = vec4<f32>(0.0);

    // The blast lights up the ground around it.
    let light = GROUND_LIGHT * exp(-age * 3.0) * exp(-dist * dist / (ring_half * ring_half) * GROUND_LIGHT_FALLOFF);
    out = vec4<f32>(out.rgb + LIGHT_COLOR * light, out.a);

    // Expanding fire ring with burning splatter behind it.
    let ring_phase = clamp(age / RING_TIME, 0.0, 1.0);
    let ring_radius = ring_half * (1.0 - pow(1.0 - ring_phase, 3.0));
    let splat_noise = gradient_fbm_2d(q * SPLAT_SCALE + vec2<f32>(-5.0, 17.0), 4);
    let flicker = gradient_noise_2d(q * 3.0 + vec2<f32>(0.0, time * 4.0));
    let splat_threshold = 0.6 + 0.15 * clamp(age / SPLAT_LIFE, 0.0, 1.0);
    let splat_region = (1.0 - smoothstep(ring_radius - 0.1, ring_radius + 0.05, m)) * (1.0 - smoothstep(ring_half - 0.2, ring_half + 0.1, m));
    let splat = smoothstep(splat_threshold, splat_threshold + 0.04, splat_noise + (flicker - 0.5) * 0.08)
        * splat_region * SPLAT * (1.0 - smoothstep(SPLAT_LIFE * 0.4, SPLAT_LIFE, age)) * ground;
    out = premultiplied_over(out, fire_ramp(0.45 + 0.15 * flicker), splat);

    let lick = gradient_fbm_2d(dir * RING_LICKS + vec2<f32>(time * 2.0, impact.seed), 3);
    let thickness = RING_THICKNESS * (0.6 + 0.8 * lick);
    let behind_front = m - ring_radius;
    let ring_mask = smoothstep(-thickness, 0.0, behind_front) * (1.0 - smoothstep(0.0, thickness * 0.3, behind_front));
    let ring_noise = gradient_fbm_2d(q * 2.5 - dir * age * 6.0 + vec2<f32>(3.0, -8.0 + impact.seed), 4);
    let ring_field = ring_mask + (ring_noise - 0.5) * 1.2;
    let ring_fire = smoothstep(0.5, 0.65, ring_field) * (1.0 - smoothstep(0.55, 1.0, ring_phase)) * ground;
    let ring_heat = RING_HEAT + 0.3 * (ring_field - 0.5) + 0.2 * (1.0 - ring_phase);
    out = premultiplied_over(out, fire_ramp(clamp(ring_heat, 0.0, 0.95)), clamp(ring_fire, 0.0, 1.0));

    // Main blast expands, breaks into patches, and fades to smoke.
    let life = age / CORE_LIFE;
    let core_radius = main_half * CORE_FILL * (1.0 - exp(-age * CORE_GROW));
    let noise_point = q * NOISE_SCALE / max(main_half, 0.5) - dir * age * BILLOW_SPEED + vec2<f32>(impact.seed * 7.0);
    let swirl = vec2<f32>(
        gradient_fbm_2d(noise_point * 0.6 + vec2<f32>(5.0 + time * 0.7, 1.0), 2),
        gradient_fbm_2d(noise_point * 0.6 + vec2<f32>(-3.0, 9.0 - time * 0.9), 2),
    ) - 0.5;
    let density = gradient_fbm_2d(noise_point + swirl * 0.9 + vec2<f32>(31.0, 7.0), 4);
    let envelope = 1.0 - m / max(core_radius, 1e-3);
    let field = density + BIAS * (envelope - 0.25) - BURN * smoothstep(0.25, 1.0, life);
    let fire = smoothstep(THRESHOLD, THRESHOLD + 0.1, field);
    let smoke_field = density + BIAS * (envelope - 0.25) - BURN * 0.5 * smoothstep(0.4, 1.6, life);
    let smoke = smoothstep(THRESHOLD - 0.18, THRESHOLD, smoke_field) * SMOKE * (1.0 - smoothstep(1.0, 1.7, life));
    let veins = 1.0 - abs(gradient_fbm_2d(noise_point * 1.8 + swirl * 0.3 + vec2<f32>(-17.0, 4.0), 3) * 2.0 - 1.0);
    let heat = mix(HEAT_START, HEAT_END, smoothstep(0.0, 0.8, life)) + 0.4 * max(field - THRESHOLD, 0.0) + 0.2 * (veins - 0.5);
    let smoke_color = mix(fire_ramp(0.22), vec3<f32>(0.07, 0.06, 0.06), 0.6); // dark smoky red
    out = premultiplied_over(out, smoke_color, smoke);
    out = premultiplied_over(out, fire_ramp(clamp(heat, 0.0, 0.98)), fire * (1.0 - 0.4 * (1.0 - veins)));

    // Flash.
    let flash = FLASH * exp(-age * FLASH_DECAY) * (1.0 - smoothstep(0.0, ring_half * 1.2, dist));
    out = vec4<f32>(out.rgb + fire_ramp(0.95) * flash, out.a);

    // Sparks flung outward, each a short streak behind its head.
    for (var i = 0u; i < SPARK_COUNT; i++) {
        let spark_hash = hash_mix((i * HASH_GOLDEN) ^ bitcast<u32>(impact.seed));
        let angle = hash_unit(spark_hash) * TAU;
        let speed_roll = hash_unit(hash_mix(spark_hash));
        let life_roll = hash_unit(hash_mix(hash_mix(spark_hash)));
        let spark_life = SPARK_LIFE * (0.6 + 0.4 * life_roll);
        if age > spark_life { continue; }
        let spark_dir = vec2<f32>(cos(angle), sin(angle));
        let reach = ring_half * SPARK_REACH * (0.5 + 0.5 * speed_roll);
        let travel = reach * (1.0 - exp(-age * 5.0 / spark_life)) / (1.0 - exp(-5.0));
        let streak = 0.25 * (1.0 - age / spark_life) + 0.03;
        let to_head = q - spark_dir * travel;
        let along_streak = clamp(dot(to_head, -spark_dir), 0.0, streak);
        let spark = (1.0 - smoothstep(0.0, 0.05, length(to_head + spark_dir * along_streak))) * (1.0 - age / spark_life);
        out = vec4<f32>(out.rgb + fire_ramp(0.85) * spark * 1.5, out.a);
    }
    return out;
}

@fragment
fn fragment(mesh: VertexOutput) -> @location(0) vec4<f32> {
    let mesh_half_size = f32(impact.reach) + 0.5;
    let q = vec2<f32>(mesh.uv.x - 0.5, 0.5 - mesh.uv.y) * 2.0 * mesh_half_size;
    let ground = 1.0 - raised(q);
    // Round fade inside the mesh, so its square outline never shows.
    let edge = 1.0 - smoothstep(mesh_half_size - 0.5, mesh_half_size, length(q));

    var out: vec4<f32>;
    if impact.layer == LAYER_SCORCH {
        out = scorch_layer(q, impact.age, ground);
    } else {
        out = blast_layer(q, impact.age, globals.time + impact.seed, ground);
    }
    return out * edge;
}
