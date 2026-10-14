import bevy_sprite_render::mesh2d::vertex_output::VertexOutput;
import package::shaders::common::fire_ramp::fire_ramp;
import package::shaders::common::value_noise::value_fbm_3d;
import package::shaders::weaponry::flame_stream_common::TILE_HALF;

// `FlameScorch` ground decal shader (world units relative to tile center).
// Premultiplied output: char darkens the ground via `alpha` while cinders add light (`rgb`).

// Field order and types mirror `FlameScorchUniform` in weaponry_internal/src/flame_stream_fire.rs.
struct FlameScorch {
    life: f32,       // normalized age (0..1)
    strength: f32,   // normalized char intensity (0..1) from burn duration
    seed: f32,
    half_size: f32,  // quad half-size in world units
};

@group(2) @binding(0)
var<uniform> scorch: FlameScorch;

const DARKNESS: f32 = 0.5;
const CINDER_DECAY: f32 = 6.0; // exponential decay rate over `life`

@fragment
fn fragment(mesh: VertexOutput) -> @location(0) vec4<f32> {
    let p = (vec2<f32>(mesh.uv.x, 1.0 - mesh.uv.y) - vec2<f32>(0.5)) * 2.0 * scorch.half_size;
    let noise = value_fbm_3d(vec3<f32>(p / 4.0, scorch.seed), 4);
    let tile_distance = max(abs(p.x), abs(p.y));
    let mark = (1.0 - smoothstep(TILE_HALF - 4.0, TILE_HALF + 2.0, tile_distance + (noise - 0.5) * 8.0))
        * smoothstep(0.35, 0.6, noise + 0.15);
    let fade = 1.0 - smoothstep(0.3, 1.0, scorch.life);
    let alpha = mark * scorch.strength * DARKNESS * fade;

    let cinder_noise = value_fbm_3d(vec3<f32>(p / 2.5, scorch.seed + 5.0), 3);
    let cinders = smoothstep(0.66, 0.72, cinder_noise) * mark * scorch.strength * exp(-scorch.life * CINDER_DECAY);
    return vec4<f32>(fire_ramp(0.55) * cinders, alpha);
}
