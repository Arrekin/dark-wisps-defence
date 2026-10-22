// Crucible field dome: a dome of heat. `displace` bends the scene beneath it with a haze; `overlay`
// adds hair-thin fire strands around its limb, a soft glow fading inward from its boundary, thin
// tendrils over the shell and drifting embers. The fire is drawn where the haze samples the scene,
// so it bends with it.
//
// The shared field index clips both passes to the field's territory. Where another field presses
// in, the seam burns like the rim.

import bevy_sprite_render::mesh2d::{vertex_output::VertexOutput, view_bindings::globals};
import package::shaders::common::displacement::world_offset_to_uv;
import package::shaders::common::fire_ramp::fire_ramp;
import package::shaders::common::hash::{HASH_GOLDEN, hash_coords, hash_mix, hash_unit};
import package::shaders::common::map_light::MAP_SUN_DIRECTION;
import package::shaders::common::value_noise::{value_fbm_3d, value_noise_3d};
import package::shaders::fields::fields::{FieldShape, field_boundary, field_owners_at, field_shapes};

// Field order and types mirror `CrucibleFieldDome` in fields_internal/src/crucible_field.rs.
struct Instance {
    radius: f32,
    progress: f32,
    seed: f32,
    // Index slot containing the field's centre and current radius.
    slot: u32,
};

// Haze
const HAZE_STRENGTH: f32 = 3.5;      // fine shimmer, world px
const HAZE_SCALE: f32 = 12.0;
const HAZE_SPEED: f32 = 2.2;
const HAZE_WAVES: f32 = 6.0;         // large slow waves, world px
const HAZE_WAVE_SCALE: f32 = 70.0;
const HAZE_WAVE_SPEED: f32 = 0.6;
const HAZE_INTERIOR: f32 = 0.85;     // haze in the middle, of the rim's
const HAZE_EDGE_FADE: f32 = 10.0;    // px over which the haze fades in from the boundary
const LENS: f32 = 3.0;               // outward lens toward the edge, px

// Dome
const DOME_SHADING: f32 = 0.6;
const SHELL_THICKNESS: f32 = 0.2;    // of radius

// Limb strands
const STRAND_LENGTH: f32 = 50.0;     // px around the dome
const STRAND_WIDTH: f32 = 4.0;       // px of in-and-out wiggle
const STRAND_SHARPNESS: f32 = 30.0;
const STRAND_SPIN: f32 = 0.04;       // rad/s
const STRAND_CHURN: f32 = 0.2;
const LIMB_DEPTH: f32 = 7.0;         // px the strands reach inward
const STRAND_HEAT: f32 = 0.55;
const STRAND_CORE_HEAT: f32 = 0.25;
const STRAND_HOT_RIM: f32 = 0.25;
const STRAND_FLECKS: f32 = 0.25;
const STRAND_BRIGHT: f32 = 1.8;

// Limb glow
const LIMB_GLOW: f32 = 0.18;
const LIMB_GLOW_DEPTH: f32 = 18.0;
const LIMB_GLOW_HEAT: f32 = 0.5;

// Shell tendrils
const INTERIOR_TENDRILS: f32 = 0.3;
const TENDRIL_HEAT: f32 = 0.55;
const TENDRIL_BRIGHT: f32 = 1.1;
const TENDRIL_SHARPNESS: f32 = 22.0;
const TENDRIL_CELL: f32 = 60.0;
const TENDRIL_RISE: f32 = 18.0;
const TENDRIL_WARP: f32 = 1.3;
const TENDRIL_SPARSE: f32 = 0.5;
const TENDRIL_INTERIOR: f32 = 0.12;

// Embers
const EMBER_CELL: f32 = 24.0;
const EMBER_CHANCE: f32 = 0.35;
const EMBER_LIFE: f32 = 1.4;
const EMBER_SIZE: f32 = 2.2;
const EMBER_BRIGHT: f32 = 4.0;
const EMBER_DRIFT: f32 = 8.0;
const EMBER_RIM_BIAS: f32 = 0.6;
const EMBER_SALT: u32 = 0x510e527fu;

// Time walks diagonally through the noise lattice so pixels do not all cross lattice planes, where
// value noise eases, at the same moment.
const TIME_WALK: vec3<f32> = vec3<f32>(0.37, 0.23, 1.0);

fn churn_noise(q: vec2<f32>, z: f32) -> f32 {
    let walk = TIME_WALK * z;
    return (value_noise_3d(vec3<f32>(q, 0.0) + walk) + 0.5 * value_noise_3d(vec3<f32>(q * 2.1 + vec2<f32>(4.3, 1.7), 0.0) + walk * 1.7)) / 1.5;
}

// ── Place ────────────────────────────────────────────────────────────────────

// Where a point sits in the field: whether the field owns it, its distance to the field's boundary
// in px (0 at the rim or at a seam), and its distance from the centre relative to the radius.
struct Place {
    mine: bool,
    edge: f32,
    relative_distance: f32,
};

fn owns(world_position: vec2<f32>, field: Instance) -> bool {
    return field_owners_at(world_position).first.slot == field.slot;
}

fn place_of(world_position: vec2<f32>, field: Instance, shape: FieldShape) -> Place {
    if !owns(world_position, field) {
        return Place(false, 0.0, 1.0);
    }
    let edge = field_boundary(field.slot, world_position).edge;
    return Place(true, edge, distance(world_position, shape.center) / shape.current_radius);
}

// ── Haze ─────────────────────────────────────────────────────────────────────

// Sampling offset in world px: a fine shimmer and large slow waves, plus a lens toward the edge.
fn haze_offset(world_position: vec2<f32>, field: Instance, shape: FieldShape, time: f32) -> vec2<f32> {
    let here = place_of(world_position, field, shape);
    if !here.mine {
        return vec2<f32>(0.0);
    }
    let from_center = world_position - shape.center;
    let direction = from_center / max(length(from_center), 0.001);
    let edge_fade = smoothstep(0.0, HAZE_EDGE_FADE, here.edge);
    let toward_rim = smoothstep(0.4, 1.0, here.relative_distance);
    let spread = mix(HAZE_INTERIOR, 1.0, toward_rim) * edge_fade;
    let q = world_position / HAZE_SCALE;
    let z = HAZE_SPEED * time + field.seed;
    let shimmer = vec2<f32>(churn_noise(q, z), churn_noise(q + vec2<f32>(19.0, 7.0), z)) * 2.0 - 1.0;
    let wave_q = world_position / HAZE_WAVE_SCALE;
    let wave_z = HAZE_WAVE_SPEED * time + field.seed + 50.0;
    let waves = vec2<f32>(churn_noise(wave_q, wave_z), churn_noise(wave_q + vec2<f32>(31.0, 13.0), wave_z)) * 2.0 - 1.0;
    return ((shimmer * HAZE_STRENGTH + waves * HAZE_WAVES) * spread - direction * LENS * toward_rim * edge_fade) * field.progress;
}

// ── Fire ─────────────────────────────────────────────────────────────────────

// Shading of the hemisphere surface above a point, lit by the map sun.
fn dome_shade(normal: vec3<f32>) -> f32 {
    return mix(1.0, 0.45 + 0.9 * max(dot(normal, MAP_SUN_DIRECTION), 0.0), DOME_SHADING);
}

// Thin tendrils on the hemisphere, crawling up toward the apex, densest where the eye looks through the
// most shell.
fn shell_tendrils(normal: vec3<f32>, path: f32, radius: f32, time: f32, seed: f32) -> vec3<f32> {
    let surface = normal * radius / TENDRIL_CELL - vec3<f32>(0.0, 0.0, TENDRIL_RISE * time / TENDRIL_CELL) + vec3<f32>(seed);
    let warp = vec3<f32>(
        value_fbm_3d(surface * 0.5 + vec3<f32>(3.1, 1.7, 0.0), 2),
        value_fbm_3d(surface * 0.5 + vec3<f32>(-5.2, 8.3, 2.0), 2),
        0.5,
    ) - vec3<f32>(0.5);
    let n = value_fbm_3d(surface + warp * TENDRIL_WARP, 3);
    let strand = pow(1.0 - abs(2.0 * n - 1.0), TENDRIL_SHARPNESS);
    let sparse = smoothstep(TENDRIL_SPARSE, TENDRIL_SPARSE + 0.2, value_fbm_3d(surface * 0.35 + vec3<f32>(9.0, 4.0, 0.0), 2));
    return fire_ramp(TENDRIL_HEAT + 0.15 * strand) * strand * sparse * max(path, TENDRIL_INTERIOR) * TENDRIL_BRIGHT;
}

// Strands of fire running around the limb, where the eye looks along the shell edge-on. Three layers
// drift around the dome in opposite directions and wiggle in and out, fading inward from the boundary.
fn limb_strands(direction: vec2<f32>, edge: f32, radius: f32, time: f32, seed: f32) -> f32 {
    var total = 0.0;
    for (var k = 0; k < 3; k++) {
        let layer = f32(k);
        let angle = time * STRAND_SPIN * select(1.0 + 0.4 * layer, -1.3, k == 1);
        let c = cos(angle);
        let s = sin(angle);
        let turned = vec2<f32>(c * direction.x - s * direction.y, s * direction.x + c * direction.y);
        let n = value_fbm_3d(vec3<f32>(turned * (radius / STRAND_LENGTH) + vec2<f32>(layer * 17.0), edge / STRAND_WIDTH + STRAND_CHURN * time + seed + layer * 5.3), 2);
        let strand = pow(1.0 - abs(2.0 * n - 1.0), STRAND_SHARPNESS);
        total += strand * exp(-edge / (LIMB_DEPTH * (1.0 + 0.6 * layer)));
    }
    return total;
}

// Sparse sparks, each cell holding at most one at a time, drifting outward as they fade.
fn embers(world_position: vec2<f32>, field: Instance, shape: FieldShape, time: f32) -> vec3<f32> {
    var color = vec3<f32>(0.0);
    let home = floor(world_position / EMBER_CELL);
    for (var dy = -1; dy <= 1; dy++) {
        for (var dx = -1; dx <= 1; dx++) {
            let cell = home + vec2<f32>(f32(dx), f32(dy));
            let h = hash_coords(cell + vec2<f32>(field.seed), EMBER_SALT);
            let period = EMBER_LIFE * (1.2 + 0.8 * hash_unit(h));
            let shift = hash_unit(hash_mix(h + 1u)) * period;
            let slot = floor((time + shift) / period);
            let slot_hash = hash_mix(h ^ (u32(i32(slot)) * HASH_GOLDEN));
            let base = (cell + 0.1 + 0.8 * vec2<f32>(hash_unit(hash_mix(slot_hash + 3u)), hash_unit(hash_mix(slot_hash + 4u)))) * EMBER_CELL;
            let relative_distance = distance(base, shape.center) / shape.current_radius;
            let chance = EMBER_CHANCE * mix(1.0, smoothstep(0.3, 1.0, relative_distance) * 1.6, EMBER_RIM_BIAS);
            if hash_unit(slot_hash) > chance { continue; }
            let life = EMBER_LIFE * (0.6 + 0.4 * hash_unit(hash_mix(slot_hash + 1u)));
            let start = slot * period - shift + hash_unit(hash_mix(slot_hash + 2u)) * (period - life);
            let age = time - start;
            let fraction = age / life;
            if fraction < 0.0 || fraction >= 1.0 { continue; }
            if !owns(base, field) { continue; }
            let outward = normalize(base - shape.center + vec2<f32>(0.001));
            let position = base + outward * EMBER_DRIFT * age;
            let twinkle = 0.6 + 0.4 * sin(age * 23.0 + hash_unit(hash_mix(slot_hash + 5u)) * 6.28);
            let size = EMBER_SIZE * (1.0 - 0.5 * fraction);
            let k = smoothstep(size, 0.0, length(world_position - position));
            color += fire_ramp(0.8 - 0.25 * fraction) * k * k * twinkle * (1.0 - fraction * fraction) * EMBER_BRIGHT;
        }
    }
    return color;
}

// Light added at `world_position`, drawn where the haze samples the scene.
fn fire_light(world_position: vec2<f32>, field: Instance, shape: FieldShape, time: f32) -> vec3<f32> {
    let seen = world_position + haze_offset(world_position, field, shape, time);
    let place = place_of(seen, field, shape);
    if !place.mine {
        return vec3<f32>(0.0);
    }
    let radius = shape.current_radius;
    let from_center = seen - shape.center;
    let distance_from_center = max(length(from_center), 0.001);
    let direction = from_center / distance_from_center;
    let edge = place.edge;

    let r = min(distance_from_center / radius, 0.999);
    let height = sqrt(1.0 - r * r);
    let normal = vec3<f32>(direction * r, height);
    let inner = 1.0 - SHELL_THICKNESS;
    let path = clamp((height - sqrt(max(inner * inner - r * r, 0.0))) / sqrt(max(1.0 - inner * inner, 0.0001)), 0.0, 1.0);
    let shade = dome_shade(normal);

    // A soft glow fading inward from the boundary, like the edge of a sphere.
    var light = fire_ramp(LIMB_GLOW_HEAT) * exp(-edge / LIMB_GLOW_DEPTH) * LIMB_GLOW * shade;

    // Strands get hotter toward their cores and the edge, with hot flecks, capped like the other fire.
    let strands = limb_strands(direction, edge, radius, time, field.seed);
    let strand_mask = min(strands, 1.0);
    let flecks = STRAND_FLECKS * smoothstep(0.72, 0.9, value_noise_3d(vec3<f32>(seen / 6.0, time * 1.5 + field.seed)));
    let strand_heat = min(STRAND_HEAT + STRAND_CORE_HEAT * strand_mask + STRAND_HOT_RIM * exp(-edge / LIMB_DEPTH) + flecks * strand_mask, 0.97);
    light += fire_ramp(strand_heat) * shade * strands * STRAND_BRIGHT;

    light += shell_tendrils(normal, path, radius, time, field.seed) * INTERIOR_TENDRILS * shade;
    light += embers(seen, field, shape, time);
    return light * field.progress;
}

// ── Parts ────────────────────────────────────────────────────────────────────

fn displace(in: VertexOutput, field: Instance) -> vec4<f32> {
    let shape = field_shapes[field.slot];
    if shape.current_radius <= 0.0 {
        return vec4<f32>(0.0);
    }
    let offset = haze_offset(in.world_position.xy, field, shape, globals.time);
    return vec4<f32>(world_offset_to_uv(offset), 0.0, 0.0);
}

fn overlay(in: VertexOutput, field: Instance) -> vec4<f32> {
    let shape = field_shapes[field.slot];
    if shape.current_radius <= 0.0 {
        return vec4<f32>(0.0);
    }
    return vec4<f32>(fire_light(in.world_position.xy, field, shape, globals.time), 0.0);
}
