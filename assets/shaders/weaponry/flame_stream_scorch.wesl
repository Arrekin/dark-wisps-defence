import bevy_sprite_render::mesh2d::vertex_output::VertexOutput;
import package::shaders::common::clock::{ClockInstant, game_time_since};
import package::shaders::common::core::{CELL_HALF, CELL_SIZE};
import package::shaders::common::fire_ramp::fire_ramp;
import package::shaders::common::noise::noise_fbm_3d;

// `FlameScorch` floor decal shader (world units relative to tile center).
// The quad covers the burnt tile, and the mark's ragged edge fades out before the tile's edge.
// Premultiplied output: char darkens the floor via `alpha` while cinders add light (`rgb`).

// Field order and types mirror `FlameScorch` in weaponry_internal/src/flame_stream_fire.rs.
struct Instance {
    start: ClockInstant, // game clock instant the tile stopped burning
    duration: f32,       // seconds the scorch lives
    strength: f32,       // normalized char intensity (0..1) from burn duration
    seed: f32,
};

const DARKNESS: f32 = 0.5;
const CINDER_DECAY: f32 = 6.0; // exponential decay rate over the normalized age

fn look(mesh: VertexOutput, scorch: Instance) -> vec4<f32> {
    let life = saturate(game_time_since(scorch.start) / scorch.duration); // normalized age (0..1)
    let p = (vec2<f32>(mesh.uv.x, 1.0 - mesh.uv.y) - vec2<f32>(0.5)) * CELL_SIZE;
    let noise = noise_fbm_3d(vec3<f32>(p / 4.0, scorch.seed), 4);
    let tile_distance = max(abs(p.x), abs(p.y));
    let mark = (1.0 - smoothstep(CELL_HALF - 6.0, CELL_HALF - 2.0, tile_distance + (noise - 0.5) * 4.0))
        * smoothstep(0.35, 0.6, noise + 0.15);
    let fade = 1.0 - smoothstep(0.3, 1.0, life);
    let alpha = mark * scorch.strength * DARKNESS * fade;

    let cinder_noise = noise_fbm_3d(vec3<f32>(p / 2.5, scorch.seed + 5.0), 3);
    let cinders = smoothstep(0.66, 0.72, cinder_noise) * mark * scorch.strength * exp(-life * CINDER_DECAY);
    return vec4<f32>(fire_ramp(0.55) * cinders, alpha);
}
