import bevy_sprite_render::mesh2d::vertex_output::VertexOutput;
import bevy_sprite_render::mesh2d::view_bindings::globals;
import package::shaders::common::clock::{ClockInstant, game_time_since};
import package::shaders::common::core::TAU;
import package::shaders::common::fire_ramp::fire_ramp;
import package::shaders::common::hash::{HASH_GOLDEN, hash_mix, hash_unit};
import package::shaders::common::noise::{noise_gradient_2d_drift, noise_gradient_fbm_2d, noise_gradient_fbm_2d_drift};

// Fireball impact shader rendering two layers on a 7x7-cell quad centered on the landing cell:
// - Blast (layer 0, drawn above wisps): flash, expanding fireball and smoke, shockwave ring,
//   splatter, barrier flare, and sparks.
// - Scorch (layer 1, drawn below wisps): fading burn mark with cooling cinders.
//
// Per-cell bitmasks (`ReachMasks`) over the 5x5 blast area control spatial masking:
// - `passed`: cells reached by the blast. Fire, smoke, ring, and sparks are confined to `passed`
//   cells and attenuated over cells that are `passed` but not `hit` (e.g. open pits).
// - `hit`: reached cells with a floor at the blast's level. Scorch only marks `hit` cells.
// - `open`: unblocked cells. Where `passed` meets a blocked (`!open`) neighbor, the expanding
//   ring piles up and flares against the barrier.
//
// Coordinates `q` are in grid cells from the landing cell's center. Output uses premultiplied
// alpha to combine additive light/sparks (`a = 0`) with alpha-blended fire and smoke.

// 25-bit masks over the 5x5 blast area, indexed row-major from bottom-left:
// bit `(y + 2) * 5 + (x + 2)` corresponds to cell offset `(x, y)` from the landing cell.
// Field order and types mirror `ImpactReachMasks` in `weaponry_internal/src/fireball.rs`.
struct ReachMasks {
    passed: u32,  // cells reached by the blast (`Passed` or `Hit`)
    hit: u32,     // reached cells with a floor at the blast's level (`Hit`)
    open: u32,    // unblocked cells at the blast's level (`!Blocked`)
};

// Field order and types mirror `FireballImpact` in weaponry_internal/src/fireball.rs.
struct Instance {
    start: ClockInstant,  // game clock instant of the impact
    seed: f32,
    layer: u32,           // 0 = blast, 1 = scorch
    duration: f32,        // layer lifetime in seconds
    reach_masks: ReachMasks,
};

// Active instance for the current fragment invocation, populated by `look`.
var<private> impact: Instance;

const LAYER_SCORCH: u32 = 1u;

// ── Area ─────────────────────────────────────────────────────────────────────
// Half-sizes in cells; paired with `MAIN_BLAST_REACH`, `RING_REACH`, and `IMPACT_REACH` in `fireball.rs`.
const AREA_RADIUS: i32 = 2;       // `RING_REACH`
const AREA_SIDE: i32 = 2 * AREA_RADIUS + 1;
const MAIN_HALF_SIZE: f32 = 1.5;  // main blast half-size in cells: `MAIN_BLAST_REACH` + 0.5
const RING_HALF_SIZE: f32 = 2.5;  // outer ring half-size in cells: `AREA_RADIUS` + 0.5
const MESH_HALF_SIZE: f32 = 3.5;  // mesh half-size in cells: `IMPACT_REACH` + 0.5

// ── Main blast ───────────────────────────────────────────────────────────────
const CORE_FILL: f32 = 1.15;      // fireball radius, relative to the main blast half-size
const CORE_GROW: f32 = 18.0;
const CORE_LIFE: f32 = 0.7;       // seconds
const SQUARENESS: f32 = 3.0;      // 2 = round, higher fills the square area
const NOISE_SCALE: f32 = 2.2;
const BILLOW_SPEED: f32 = 1.5;
const BIAS: f32 = 1.0;
const BURN: f32 = 0.9;            // erosion rate of the fireball into clumps
const THRESHOLD: f32 = 0.5;
const HEAT_START: f32 = 0.98;
const HEAT_END: f32 = 0.55;
const SMOKE: f32 = 0.42;
const FLASH: f32 = 1.2;
const FLASH_DECAY: f32 = 14.0;
const GROUND_LIGHT: f32 = 0.3;
const GROUND_LIGHT_FALLOFF: f32 = 2.5; // exponential decay coefficient for (distance / `RING_HALF_SIZE`)^2

// ── Ring ─────────────────────────────────────────────────────────────────────
const RING_TIME: f32 = 0.35;      // expansion duration to `RING_HALF_SIZE` (seconds)
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

// ── Spread ───────────────────────────────────────────────────────────────────
const EDGE_SOFTNESS: f32 = 0.2;        // smoothstep half-width around the 0.5 mask boundary
const EDGE_NOISE: f32 = 0.3;           // noise amplitude perturbing the mask boundary
const BARRIER_FLARE: f32 = 1.6;        // intensity of fire piling up against a barrier
const BARRIER_FLARE_WIDTH: f32 = 0.35; // band width inside the `passed` boundary where flare builds
const BARRIER_FLARE_LIFE: f32 = 0.7;   // decay time (seconds) after the ring reaches the barrier
const SHADED_LIGHT: f32 = 0.35;        // light and flash multiplier on unreached cells
const PASSED_FIRE: f32 = 0.4;          // fireball opacity multiplier over floorless `passed` cells
const PASSED_SMOKE: f32 = 0.6;         // smoke opacity multiplier over floorless `passed` cells
const PASSED_GROUND: f32 = 0.4;        // ring and splatter opacity multiplier over floorless `passed` cells

const LIGHT_COLOR: vec3<f32> = vec3<f32>(1.0, 0.45, 0.15); // warm orange

// ── Reach lookup ─────────────────────────────────────────────────────────────
// Bit for `offset` in a 5x5 `ReachMasks` field. Offsets in the 7x7 mesh margin clamp to the
// nearest 5x5 rim cell so open edges fade naturally to the mesh boundary instead of clipping at 5x5.
fn area_bit(offset: vec2<i32>) -> u32 {
    let nearest = clamp(offset, vec2<i32>(-AREA_RADIUS), vec2<i32>(AREA_RADIUS));
    return 1u << u32((nearest.y + AREA_RADIUS) * AREA_SIDE + nearest.x + AREA_RADIUS);
}

fn mask_cell(offset: vec2<i32>, mask: u32) -> f32 {
    return f32((mask & area_bit(offset)) != 0u);
}

// Bilinearly interpolates `mask` at cell-space position `q` (cell centers at integers, edges at 0.5).
fn mask_field(q: vec2<f32>, mask: u32) -> f32 {
    let base = floor(q);
    let f = q - base;
    let cell = vec2<i32>(base);
    let bottom = mix(mask_cell(cell, mask), mask_cell(cell + vec2<i32>(1, 0), mask), f.x);
    let top = mix(mask_cell(cell + vec2<i32>(0, 1), mask), mask_cell(cell + vec2<i32>(1, 1), mask), f.x);
    return mix(bottom, top, f.y);
}

// Thresholds a bilinear mask field at 0.5 (the cell boundary) with noise-perturbed soft edges.
fn spread_coverage(field: f32, ragged: f32) -> f32 {
    return smoothstep(0.5 - EDGE_SOFTNESS, 0.5 + EDGE_SOFTNESS, field + (ragged - 0.5) * EDGE_NOISE);
}

// Superellipse norm: p = 2 is a circle, large p approaches a square.
fn shape_norm(q: vec2<f32>, p: f32) -> f32 {
    let a = abs(q) + vec2<f32>(1e-5);
    return pow(pow(a.x, p) + pow(a.y, p), 1.0 / p);
}

fn premultiplied_over(below: vec4<f32>, color: vec3<f32>, alpha: f32) -> vec4<f32> {
    return vec4<f32>(color * alpha + below.rgb * (1.0 - alpha), alpha + below.a * (1.0 - alpha));
}

fn scorch_layer(q: vec2<f32>, age: f32, hit: f32) -> vec4<f32> {
    let main_blast = shape_norm(q, SQUARENESS) / (MAIN_HALF_SIZE * SCORCH_SIZE);
    let edge_noise = noise_gradient_fbm_2d(q * 2.0 + vec2<f32>(13.0, 3.0), 3);
    let scorch = (1.0 - smoothstep(0.7, 1.0, main_blast + (edge_noise - 0.5) * 0.8))
        * SCORCH * smoothstep(0.0, 0.2, age) * (1.0 - smoothstep(impact.duration * 0.4, impact.duration, age)) * hit;
    let cinders = smoothstep(0.66, 0.72, noise_gradient_fbm_2d(q * 5.0 + vec2<f32>(7.0, 2.0), 3)) * scorch * exp(-age * CINDER_DECAY);
    return vec4<f32>(fire_ramp(0.6) * cinders * 1.5, scorch * 0.75);
}

// Time in seconds when the expanding ring reaches superellipse distance `m`, or -1.0 if beyond `RING_HALF_SIZE`.
fn ring_arrival(m: f32) -> f32 {
    let reached = m / RING_HALF_SIZE;
    if reached >= 1.0 {
        return -1.0;
    }
    return (1.0 - pow(1.0 - reached, 1.0 / 3.0)) * RING_TIME;
}

fn blast_layer(q: vec2<f32>, age: f32, time: f32) -> vec4<f32> {
    let m = shape_norm(q, SQUARENESS);
    let dist = length(q);
    let dir = q / max(dist, 1e-3);

    // Spread coverage: `passed` where the blast travels, `hit` where it touches a floor.
    let ragged = noise_gradient_fbm_2d_drift(q * 2.2 + vec2<f32>(impact.seed, 11.0), time, vec2<f32>(0.0, 1.5), 3);
    let passed_field = mask_field(q, impact.reach_masks.passed);
    let passed = spread_coverage(passed_field, ragged);
    let hit = spread_coverage(mask_field(q, impact.reach_masks.hit), ragged) * passed;
    let shaded = mix(SHADED_LIGHT, 1.0, passed);
    // 1.0 on hit cells, 0.0 on reached cells without a floor (where fire/smoke/ring are attenuated).
    let over_floor = mix(1.0, hit, passed);
    let ground = mix(PASSED_GROUND, 1.0, over_floor) * passed;

    // Ground light pool.
    let light = GROUND_LIGHT * exp(-age * 3.0) * exp(-dist * dist / (RING_HALF_SIZE * RING_HALF_SIZE) * GROUND_LIGHT_FALLOFF) * shaded;
    var out = vec4<f32>(LIGHT_COLOR * light, 0.0);

    // Expanding fire ring with burning splatter behind it.
    let ring_phase = clamp(age / RING_TIME, 0.0, 1.0);
    let ring_radius = RING_HALF_SIZE * (1.0 - pow(1.0 - ring_phase, 3.0));
    let splat_noise = noise_gradient_fbm_2d(q * SPLAT_SCALE + vec2<f32>(-5.0, 17.0), 4);
    let noise_point = q * NOISE_SCALE / MAIN_HALF_SIZE - dir * age * BILLOW_SPEED + vec2<f32>(impact.seed);
    let flicker = noise_gradient_2d_drift(q * 3.0, time, vec2<f32>(0.0, 4.0));
    let splat_threshold = 0.6 + 0.15 * clamp(age / SPLAT_LIFE, 0.0, 1.0);
    let splat_region = (1.0 - smoothstep(ring_radius - 0.1, ring_radius + 0.05, m)) * (1.0 - smoothstep(RING_HALF_SIZE - 0.2, RING_HALF_SIZE + 0.1, m));
    let splat = smoothstep(splat_threshold, splat_threshold + 0.04, splat_noise + (flicker - 0.5) * 0.08)
        * splat_region * SPLAT * (1.0 - smoothstep(SPLAT_LIFE * 0.4, SPLAT_LIFE, age)) * ground;
    out = premultiplied_over(out, fire_ramp(0.45 + 0.15 * flicker), splat);

    let lick = noise_gradient_fbm_2d_drift(dir * RING_LICKS + vec2<f32>(0.0, impact.seed), time, vec2<f32>(2.0, 0.0), 3);
    let thickness = RING_THICKNESS * (0.6 + 0.8 * lick);
    let behind_front = m - ring_radius;
    let ring_mask = smoothstep(-thickness, 0.0, behind_front) * (1.0 - smoothstep(0.0, thickness * 0.3, behind_front));
    let ring_noise = noise_gradient_fbm_2d(q * 2.5 - dir * age * 6.0 + vec2<f32>(3.0, -8.0 + impact.seed), 4);
    let ring_field = ring_mask + (ring_noise - 0.5) * 1.2;
    let ring_fire = smoothstep(0.5, 0.65, ring_field) * (1.0 - smoothstep(0.55, 1.0, ring_phase)) * ground;
    let ring_heat = RING_HEAT + 0.3 * (ring_field - 0.5) + 0.2 * (1.0 - ring_phase);
    out = premultiplied_over(out, fire_ramp(clamp(ring_heat, 0.0, 0.95)), clamp(ring_fire, 0.0, 1.0));

    // Barrier flare: where `passed` meets a blocked (`!open`) neighbor, the arriving ring piles up and decays.
    let arrival = ring_arrival(m);
    let since_arrival = age - arrival;
    let barrier_band = (1.0 - smoothstep(0.5 - BARRIER_FLARE_WIDTH, 1.0, passed_field)) * passed
        * smoothstep(0.15, 0.4, 1.0 - mask_field(q, impact.reach_masks.open)) * step(0.0, arrival) * step(0.0, since_arrival);
    let life = age / CORE_LIFE;
    let blast_fade = 1.0 - smoothstep(0.3, 0.85, life);
    let pile = BARRIER_FLARE * barrier_band * exp(-max(since_arrival, 0.0) / BARRIER_FLARE_LIFE) * over_floor * blast_fade;
    let pile_fire = smoothstep(0.45, 0.6, pile * 0.6 + ring_noise * 0.7 + (flicker - 0.5) * 0.2) * smoothstep(0.02, 0.25, pile);

    // Main blast expands, breaks into patches, and fades to smoke.
    let core_radius = MAIN_HALF_SIZE * CORE_FILL * (1.0 - exp(-age * CORE_GROW));
    let swirl = vec2<f32>(
        noise_gradient_fbm_2d_drift(noise_point * 0.6 + vec2<f32>(5.0, 1.0), time, vec2<f32>(0.7, 0.0), 2),
        noise_gradient_fbm_2d_drift(noise_point * 0.6 + vec2<f32>(-3.0, 9.0), time, vec2<f32>(0.0, -0.9), 2),
    ) - 0.5;
    let density = noise_gradient_fbm_2d(noise_point + swirl * 0.9 + vec2<f32>(31.0, 7.0), 4);
    let envelope = 1.0 - m / max(core_radius, 1e-3);
    let field = density + BIAS * (envelope - 0.25) - BURN * smoothstep(0.25, 1.0, life) + 0.4 * pile;
    let fire = smoothstep(THRESHOLD, THRESHOLD + 0.1, field) * passed * mix(PASSED_FIRE, 1.0, over_floor);
    let smoke_field = density + BIAS * (envelope - 0.25) - BURN * 0.5 * smoothstep(0.4, 1.6, life);
    let smoke = smoothstep(THRESHOLD - 0.18, THRESHOLD, smoke_field) * SMOKE * (1.0 - smoothstep(1.0, 1.7, life))
        * passed * mix(PASSED_SMOKE, 1.0, over_floor);
    let veins = 1.0 - abs(noise_gradient_fbm_2d(noise_point * 1.8 + swirl * 0.3 + vec2<f32>(-17.0, 4.0), 3) * 2.0 - 1.0);
    let heat = mix(HEAT_START, HEAT_END, smoothstep(0.0, 0.8, life)) + 0.4 * max(field - THRESHOLD, 0.0) + 0.2 * (veins - 0.5);
    let smoke_color = mix(fire_ramp(0.22), vec3<f32>(0.07, 0.06, 0.06), 0.6); // dark smoky red
    out = premultiplied_over(out, smoke_color, smoke);
    out = premultiplied_over(out, fire_ramp(clamp(heat, 0.0, 0.98)), fire * (1.0 - 0.4 * (1.0 - veins)));
    out = premultiplied_over(out, fire_ramp(clamp(0.72 + 0.25 * pile + 0.1 * (flicker - 0.5), 0.0, 0.97)), pile_fire);

    // Flash.
    let flash = FLASH * exp(-age * FLASH_DECAY) * (1.0 - smoothstep(0.0, RING_HALF_SIZE * 1.2, dist)) * shaded;
    out = vec4<f32>(out.rgb + fire_ramp(0.95) * flash, out.a);

    // Sparks flung outward, each a short streak behind its head; barriers stop them.
    for (var i = 0u; i < SPARK_COUNT; i++) {
        let spark_hash = hash_mix((i * HASH_GOLDEN) ^ bitcast<u32>(impact.seed));
        let angle = hash_unit(spark_hash) * TAU;
        let speed_roll = hash_unit(hash_mix(spark_hash));
        let life_roll = hash_unit(hash_mix(hash_mix(spark_hash)));
        let spark_life = SPARK_LIFE * (0.6 + 0.4 * life_roll);
        if age > spark_life { continue; }
        let spark_dir = vec2<f32>(cos(angle), sin(angle));
        let reach = RING_HALF_SIZE * SPARK_REACH * (0.5 + 0.5 * speed_roll);
        let travel = reach * (1.0 - exp(-age * 5.0 / spark_life)) / (1.0 - exp(-5.0));
        let streak = 0.25 * (1.0 - age / spark_life) + 0.03;
        let to_head = q - spark_dir * travel;
        let along_streak = clamp(dot(to_head, -spark_dir), 0.0, streak);
        let spark = (1.0 - smoothstep(0.0, 0.05, length(to_head + spark_dir * along_streak))) * (1.0 - age / spark_life);
        out = vec4<f32>(out.rgb + fire_ramp(0.85) * spark * 1.5 * passed, out.a);
    }
    return out;
}

fn look(mesh: VertexOutput, instance: Instance) -> vec4<f32> {
    impact = instance;
    let age = game_time_since(impact.start);
    let q = vec2<f32>(mesh.uv.x - 0.5, 0.5 - mesh.uv.y) * 2.0 * MESH_HALF_SIZE;
    // Radial falloff inside the quad boundary.
    let edge = 1.0 - smoothstep(MESH_HALF_SIZE - 0.5, MESH_HALF_SIZE, length(q));

    var out: vec4<f32>;
    if impact.layer == LAYER_SCORCH {
        let ragged = noise_gradient_fbm_2d(q * 2.2 + vec2<f32>(impact.seed, 11.0), 3);
        out = scorch_layer(q, age, spread_coverage(mask_field(q, impact.reach_masks.hit), ragged));
    } else {
        out = blast_layer(q, age, globals.time + impact.seed);
    }
    return out * edge;
}
