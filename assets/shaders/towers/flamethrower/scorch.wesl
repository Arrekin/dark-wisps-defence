import bevy_sprite_render::mesh2d::vertex_output::VertexOutput;
import package::shaders::common::fire_ramp::fire_ramp;
import package::shaders::common::value_noise::value_fbm_3d;
import package::shaders::towers::flamethrower::common::TILE_HALF;

// Char a Flamethrower leaves on a tile it stopped burning: a ragged dark mark with a few cooling
// cinders, fading away.
//
// Distances are in world units, measured from the tile's center. Output is premultiplied, so the
// mark darkens the ground while the cinders glow. Nothing is sampled from a texture.

// Field order and types mirror `FlameScorchUniform` in buildings_internal/src/towers/blaster/tower_flamethrower_flame.rs.
struct FlameScorch {
    life: f32,       // 0 when the burning stopped, 1 once the mark is gone
    strength: f32,   // 0..1, grows with how long the tile burned
    seed: f32,
    half_size: f32,  // mesh half-size
};

@group(2) @binding(0)
var<uniform> scorch: FlameScorch;

const DARKNESS: f32 = 0.5;
const CINDER_DECAY: f32 = 6.0; // how fast the cinders cool, relative to the mark's life

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
