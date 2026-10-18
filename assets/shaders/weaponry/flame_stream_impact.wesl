import bevy_sprite_render::mesh2d::vertex_output::VertexOutput;
import bevy_sprite_render::mesh2d::view_bindings::globals;
import package::shaders::common::fire_ramp::{FIRE_HEAT_CAP, fire_ramp};
import package::shaders::common::hash::{HASH_GOLDEN, hash_mix, hash_unit};
import package::shaders::common::value_noise::value_fbm_3d;
import package::shaders::weaponry::flame_stream_common::TILE_HALF;

// `ImpactFire` shader on a `FlameStream`'s hit tile (world units relative to tile center,
// premultiplied alpha):
// - `fan`: lateral splash at `hit_point`.
// - `cling`: downstream flame tongues around `hit_point`.
// - `tile_fire`: low flames and border rim across the `[-TILE_HALF, TILE_HALF]` cell.
// - `spark`: additive procedural particles deflecting backward/sideways from `hit_point`.

// Field order and types mirror `ImpactFireVisual` in weaponry_internal/src/flame_stream_fire.rs.
struct Instance {
    intensity: f32,         // normalized burn intensity (0..1)
    seed: f32,              // per-stream phase offset
    half_size: f32,         // quad half-size in world units
    hit_point: vec2<f32>,   // hit position relative to tile center
    direction: vec2<f32>,   // unit stream direction
};

const SPLASH: f32 = 18.0;          // lateral splash half-width

// ── Sparks ───────────────────────────────────────────────────────────────────
const SPARK_COUNT: u32 = 16u;
const SPARK_RADIUS: f32 = 1.4;
const SPARK_DRAG: f32 = 4.0;       // exponential velocity decay rate (1/s)

fn look(mesh: VertexOutput, impact: Instance) -> vec4<f32> {
    let p = (vec2<f32>(mesh.uv.x, 1.0 - mesh.uv.y) - vec2<f32>(0.5)) * 2.0 * impact.half_size;
    let time = globals.time + impact.seed;
    let direction = impact.direction;
    let normal = vec2<f32>(-direction.y, direction.x);

    // Lateral splash fan at `hit_point`.
    let from_hit = p - impact.hit_point;
    let along = dot(from_hit, direction);
    let across = dot(from_hit, normal);
    let radius = length(from_hit);
    let angle = atan2(across, along);
    let spray = value_fbm_3d(vec3<f32>(angle * 2.2, radius / 6.0 - time * 5.0, time * 1.3), 4);
    let spread = SPLASH * (0.35 + 0.65 * smoothstep(-12.0, 6.0, along));
    let fan = (1.0 - smoothstep(spread * 0.55, spread, abs(across) + (spray - 0.5) * 18.0))
        * smoothstep(-16.0, -6.0, along) * (1.0 - smoothstep(4.0, 16.0, along + (spray - 0.5) * 10.0));

    // Downstream clinging flames around `hit_point`.
    let downstream = max(along, 0.0);
    let cling_radius = length(from_hit - direction * downstream * 0.55);
    let tongues = value_fbm_3d(vec3<f32>(angle * 1.6, cling_radius / 5.0 - time * 4.0, time), 4);
    let cling = smoothstep(17.0, 7.0, cling_radius + (tongues - 0.5) * 12.0) * smoothstep(3.5, 7.5, radius);

    // Full-cell surface flames and cell border highlight.
    let tile_distance = max(abs(p.x), abs(p.y));
    let tile_noise = value_fbm_3d(vec3<f32>(p / 5.0, time * 2.4), 4);
    let tile = 1.0 - smoothstep(TILE_HALF - 3.0, TILE_HALF + 2.0, tile_distance + (tile_noise - 0.5) * 7.0);
    let tile_fire = smoothstep(0.5, 0.64, tile_noise + 0.1) * tile;

    let field = max(max(fan * smoothstep(0.38, 0.55, spray + 0.08), cling * smoothstep(0.42, 0.56, tongues + 0.1)), tile_fire);
    let heat = 0.45 + 0.4 * max(fan, cling) * spray + 0.15 * tile_fire;
    let fire_alpha = clamp(field * impact.intensity, 0.0, 1.0);
    var color = fire_ramp(min(heat, FIRE_HEAT_CAP)) * fire_alpha * 1.2;
    let rim = smoothstep(2.5, 0.0, abs(tile_distance - TILE_HALF)) * impact.intensity * 0.25 * (0.6 + 0.4 * tile_noise);
    color += fire_ramp(0.5) * rim;
    let alpha = fire_alpha * 0.85 + rim * 0.3;

    // Additive sparks with exponential drag (`v(t) = v0 * exp(-SPARK_DRAG * t)`).
    for (var i = 0u; i < SPARK_COUNT; i++) {
        let spark_hash = hash_mix((i * HASH_GOLDEN) ^ bitcast<u32>(impact.seed));
        let life = 0.2 + 0.45 * hash_unit(spark_hash);
        let cycle_time = globals.time / life + hash_unit(hash_mix(spark_hash));
        let phase = fract(cycle_time);
        let cycle_hash = hash_mix(spark_hash ^ bitcast<u32>(floor(cycle_time)));
        let sideways = (hash_unit(cycle_hash) * 2.0 - 1.0) * 150.0;
        let backward = -20.0 - hash_unit(hash_mix(cycle_hash)) * 70.0;
        let velocity = direction * backward + normal * sideways;
        let age = phase * life;
        let position = impact.hit_point + velocity * (1.0 - exp(-SPARK_DRAG * age)) / SPARK_DRAG;
        let spark = (1.0 - smoothstep(SPARK_RADIUS * 0.4, SPARK_RADIUS, length(p - position))) * (1.0 - phase) * impact.intensity;
        color += mix(fire_ramp(FIRE_HEAT_CAP), fire_ramp(0.5), phase) * spark * 1.4;
    }
    return vec4<f32>(color, alpha);
}
