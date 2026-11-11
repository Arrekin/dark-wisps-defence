// Shared shading for underground terrain and excavation previews.
// All excavations use the same earth palette.
// Colours are linear. Distances are in world pixels.

import package::shaders::common::core::CELL_SIZE;
import package::shaders::common::gradient_noise::gradient_fbm_2d;
import package::shaders::common::map_light::{MAP_SUN_ELEVATION_TANGENT, MAP_SUN_GROUND_DIRECTION};
import package::shaders::walls::look::{passage_arm_distance, ramp_up};
import package::shaders::walls::styles::{opens_toward, shape_openings, shape_ramp};

// Ground colour and exposed earth palette
const GROUND_COLOUR: vec3<f32> = vec3<f32>(0.00651, 0.00699, 0.00802);
// Earth exposed in cliff faces and around tunnel passages.
const EARTH_LIT_COLOUR: vec3<f32> = vec3<f32>(0.060, 0.048, 0.036);
const EARTH_SHADE_COLOUR: vec3<f32> = vec3<f32>(0.0030, 0.0029, 0.0032);

// Pit floor
// Floor brightness relative to the ground.
const PIT_FLOOR_TONE: f32 = 0.45;

// Cliff faces
// Width of a full-height cliff in the map plane, from rim to floor.
const FACE_WIDTH: f32 = 5.0;
// Brightness multipliers for faces toward and away from the sun.
const FACE_LIGHT: f32 = 1.0;
const FACE_SHADE: f32 = 1.0;
// Face brightness at the floor, relative to the rim.
const FACE_FOOT_TONE: f32 = 0.45;
// Floor darkening at the cliff foot, and the distance over which it fades.
const FOOT_OCCLUSION: f32 = 0.6;
const FOOT_OCCLUSION_REACH: f32 = 15.0;

// Rim
// Maximum additional rim overhang from erosion.
const RIM_OVERHANG: f32 = 2.5;
// Spatial scale of erosion noise.
const RIM_GRAIN: f32 = 7.0;
// Rim highlight strength.
const RIM_LIGHT: f32 = 0.6;
// Fraction of the highlight retained on faces away from the sun.
const RIM_SHADED_LIGHT: f32 = 0.45;
// Highlight width; also the base overhang before erosion and cliff-height scaling.
const RIM_WIDTH: f32 = 1.5;

// Sun shadow
// Depth of a pit, setting how far its rim's shadow reaches across the floor.
const PIT_DEPTH: f32 = 20.0;
const SHADOW_STRENGTH: f32 = 0.7;
// Blur of the shadow's edge.
const SHADOW_SOFTNESS: f32 = 1.5;

// Ramps
// Directional lighting strength, strongest halfway up the ramp.
const RAMP_TILT_LIGHT: f32 = 0.4;
// Directional lighting near the crest, peaking at two-thirds height and fading to zero at the top.
const RAMP_CREST_BREAK: f32 = 0.3;
// Fraction of directional lighting retained on slopes facing away from the sun.
const RAMP_SHADE_SHARE: f32 = 0.35;
// Crease shading near the ramp foot, strongest at one-third height.
// Negative values lighten the crease; positive values darken it.
const RAMP_FOOT_CREASE: f32 = -0.3;
// Earth tint strength; preserves the ramp's luminance.
const RAMP_EARTH_TINT: f32 = 0.5;

// Tunnels
// Passage floor brightness relative to the pit floor.
const PASSAGE_FLOOR_TONE: f32 = 0.6;
// Earth brightness around the passage, between the face shade and lit colours.
const INTERIOR_EARTH_TONE: f32 = 0.25;
// Lit line where the earth meets the passage.
const INTERIOR_EDGE_LIGHT: f32 = 0.6;
const INTERIOR_EDGE_WIDTH: f32 = 1.5;
// Shadow the earth casts onto the passage floor.
const PASSAGE_SHADOW_REACH: f32 = 4.0;
const PASSAGE_SHADOW_STRENGTH: f32 = 0.6;
// Tunnel mouth width in world pixels.
const MOUTH_WIDTH: f32 = 20.0;
// Arch height as a fraction of the cliff height: 0 at the floor, 1 at the rim.
const ARCH_HEIGHT: f32 = 1.0;
// Recess brightness inside an arch, relative to the passage floor.
const RECESS_TONE: f32 = 0.08;

// Cover
// Opacity of the ground over a tunnel at full reveal.
const REVEALED_COVER_OPACITY: f32 = 0.3;

fn erosion_noise(world: vec2<f32>) -> f32 {
    return saturate(gradient_fbm_2d(world / RIM_GRAIN, 3));
}

fn pit_floor_colour() -> vec3<f32> {
    return GROUND_COLOUR * PIT_FLOOR_TONE;
}

// Earth colour interpolated from shade to light; clamps `light` to [0, 1].
fn earth_colour(light: f32) -> vec3<f32> {
    return mix(EARTH_SHADE_COLOUR * FACE_SHADE, EARTH_LIT_COLOUR * FACE_LIGHT, saturate(light));
}

// Cliff colour for a unit `normal` in the map plane and `depth` from 0 (rim) to 1 (floor).
fn face_colour(normal: vec2<f32>, depth: f32) -> vec3<f32> {
    let lit = dot(normal, MAP_SUN_GROUND_DIRECTION);
    return earth_colour(0.5 + 0.5 * lit) * mix(1.0, FACE_FOOT_TONE, depth);
}

// Ramp colour at `rise` from 0 (foot) to 1 (crest), with uphill direction `up` in the map plane.
// Blends from `floor` to ground colour and applies directional lighting and an earth tint.
fn ramp_colour(floor: vec3<f32>, rise: f32, up: vec2<f32>) -> vec3<f32> {
    let sun_facing = -dot(up, MAP_SUN_GROUND_DIRECTION);
    let facing = select(sun_facing * RAMP_SHADE_SHARE, sun_facing, sun_facing > 0.0);
    // Lighting fades to zero at both ends to avoid seams with the floor and ground.
    let tilt = RAMP_TILT_LIGHT * 4.0 * rise * (1.0 - rise);
    let crest = RAMP_CREST_BREAK * 6.75 * rise * rise * (1.0 - rise);
    let shaded = mix(floor, GROUND_COLOUR, rise) * (1.0 + facing * (tilt + crest));
    let crease = RAMP_FOOT_CREASE * 6.75 * rise * (1.0 - rise) * (1.0 - rise);
    let on_slope = select(0.0, 1.0, rise > 0.0 && rise < 1.0);
    return earth_tinted(shaded, RAMP_EARTH_TINT * on_slope) * (1.0 - crease);
}

// Blends `colour` toward the earth palette by `amount`, preserving luminance.
fn earth_tinted(colour: vec3<f32>, amount: f32) -> vec3<f32> {
    let luma = vec3<f32>(0.2126, 0.7152, 0.0722);
    let earth = EARTH_LIT_COLOUR * dot(colour, luma) / dot(EARTH_LIT_COLOUR, luma);
    return mix(colour, earth, amount);
}

// Rim highlight over `surface`, the colour above the cliff. `normal` is the cliff's unit normal
// in the map plane; `distance` measures inward from the lip's edge in world pixels.
fn rim_colour(surface: vec3<f32>, normal: vec2<f32>, distance: f32) -> vec3<f32> {
    let facing = mix(RIM_SHADED_LIGHT, 1.0, saturate(dot(normal, MAP_SUN_GROUND_DIRECTION)));
    let lip = (1.0 - smoothstep(0.0, RIM_WIDTH, distance)) * facing;
    return mix(surface, EARTH_LIT_COLOUR, lip * RIM_LIGHT);
}

// A tunnel interior: earth around the passage, and the passage floor darkened by the earth's
// shadow. `distance` is the signed distance to the passage, positive in earth.
fn interior_colour(distance: f32, texel: f32) -> vec3<f32> {
    let earth = smoothstep(-texel, texel, distance);
    let edge = saturate(1.0 - distance / max(INTERIOR_EDGE_WIDTH, texel));
    let earth_shade = earth_colour(INTERIOR_EARTH_TONE + INTERIOR_EDGE_LIGHT * edge);
    let reach = saturate(1.0 + distance / PASSAGE_SHADOW_REACH);
    let shadow = reach * reach * PASSAGE_SHADOW_STRENGTH;
    let passage = pit_floor_colour() * PASSAGE_FLOOR_TONE * (1.0 - shadow);
    return mix(passage, earth_shade, earth);
}

// Ramp height from 0 (floor) to 1 (ground), at `p` in cell units from the cell centre.
// Reserve FACE_WIDTH at each end for a level landing; the slope spans the remaining distance.
fn ramp_rise(ramp: u32, p: vec2<f32>) -> f32 {
    let from_foot = (dot(p, vec2<f32>(ramp_up(ramp))) + 0.5) * CELL_SIZE;
    return saturate((from_foot - FACE_WIDTH) / max(CELL_SIZE - 2.0 * FACE_WIDTH, 1.0));
}

// Isolated cells
// Preview helpers assume solid earth at every neighbouring edge and corner.
// `p` is in cell units from the cell centre; `texel` is world pixels per screen pixel.

// Surface height of an isolated ramp, or a pit when `ramp` is zero.
fn single_cell_height(ramp: u32, p: vec2<f32>) -> f32 {
    if ramp == 0u { return 0.0; }
    return ramp_rise(ramp, p);
}

// Highest cliff surface at a point in an isolated pit or ramp.
struct SingleCellCliff {
    // Height of the cliff's surface at the point.
    height: f32,
    // Distance from the cliff's rim, in world pixels.
    distance: f32,
    // Height difference at the rim, from 0 to 1; zero when no cliff was found.
    step: f32,
    // Unit normal in the map plane, pointing into the cell.
    normal: vec2<f32>,
}

fn single_cell_cliff(ramp: u32, p: vec2<f32>) -> SingleCellCliff {
    var best = SingleCellCliff(-1e6, 1e6, 0.0, vec2<f32>(0.0));
    var cardinals = array<vec2<f32>, 4>(vec2<f32>(1.0, 0.0), vec2<f32>(-1.0, 0.0), vec2<f32>(0.0, 1.0), vec2<f32>(0.0, -1.0));
    for (var i = 0; i < 4; i++) {
        let outward = cardinals[i];
        let to_edge = 0.5 - dot(p, outward);
        let step = 1.0 - single_cell_height(ramp, p + outward * to_edge);
        if step <= 0.02 { continue; }
        let distance = to_edge * CELL_SIZE;
        let height = 1.0 - distance / FACE_WIDTH;
        if height > best.height { best = SingleCellCliff(height, distance, step, -outward); }
    }
    var diagonals = array<vec2<f32>, 4>(vec2<f32>(1.0, 1.0), vec2<f32>(-1.0, 1.0), vec2<f32>(1.0, -1.0), vec2<f32>(-1.0, -1.0));
    for (var i = 0; i < 4; i++) {
        let corner = diagonals[i] * 0.5;
        let step = 1.0 - single_cell_height(ramp, corner);
        if step <= 0.02 { continue; }
        let from_corner = p - corner;
        let distance = length(from_corner) * CELL_SIZE;
        let height = 1.0 - distance / FACE_WIDTH;
        if height > best.height { best = SingleCellCliff(height, distance, step, normalize(from_corner + vec2<f32>(1e-6))); }
    }
    return best;
}

// Strength of the shadow the surrounding ground casts on a surface at `height`.
fn single_cell_shadow(p: vec2<f32>, height: f32) -> f32 {
    let step = 1.0 - height;
    if step <= 0.02 { return 0.0; }
    let local = p * CELL_SIZE;
    let to_edges = (vec2<f32>(CELL_SIZE * 0.5) - local * sign(MAP_SUN_GROUND_DIRECTION)) / abs(MAP_SUN_GROUND_DIRECTION);
    let distance = min(to_edges.x, to_edges.y);
    if distance >= PIT_DEPTH / MAP_SUN_ELEVATION_TANGENT { return 0.0; }
    let above = step * PIT_DEPTH - distance * MAP_SUN_ELEVATION_TANGENT;
    return smoothstep(-SHADOW_SOFTNESS, SHADOW_SOFTNESS, above);
}

// Isolated pit or ramp with cliffs, cast shadows and rims. A zero `ramp` selects a pit.
fn single_cell_open(ramp: u32, p: vec2<f32>, world: vec2<f32>, texel: f32) -> vec3<f32> {
    let height = single_cell_height(ramp, p);
    let floor = pit_floor_colour();
    var colour = floor;
    if ramp != 0u {
        colour = ramp_colour(floor, height, vec2<f32>(ramp_up(ramp)));
    }
    colour *= 1.0 - SHADOW_STRENGTH * single_cell_shadow(p, height);

    let cliff = single_cell_cliff(ramp, p);
    if cliff.step <= 0.0 { return colour; }
    let beyond_foot = (height - cliff.height) * FACE_WIDTH;
    let falloff = saturate(1.0 - max(beyond_foot, 0.0) / FOOT_OCCLUSION_REACH);
    colour *= 1.0 - FOOT_OCCLUSION * cliff.step * falloff;
    let depth = saturate(cliff.distance / max(FACE_WIDTH * cliff.step, texel));
    colour = mix(face_colour(cliff.normal, depth), colour, smoothstep(-texel, texel, beyond_foot));

    let overhang = (RIM_WIDTH + RIM_OVERHANG * erosion_noise(world)) * cliff.step;
    let lip_edge = overhang - cliff.distance;
    return mix(colour, rim_colour(GROUND_COLOUR, cliff.normal, max(lip_edge, 0.0)), smoothstep(-texel, texel, lip_edge));
}

// Isolated tunnel interior under translucent ground at full reveal.
// `openings` uses the bitmask returned by `shape_openings`.
fn single_cell_tunnel(openings: u32, p: vec2<f32>, texel: f32) -> vec3<f32> {
    var sides = array<vec2<i32>, 4>(vec2<i32>(0, 1), vec2<i32>(1, 0), vec2<i32>(0, -1), vec2<i32>(-1, 0));
    var passage = 1e6;
    for (var i = 0; i < 4; i++) {
        if opens_toward(openings, sides[i]) {
            passage = min(passage, passage_arm_distance(p, vec2<f32>(sides[i])));
        }
    }
    return mix(interior_colour(passage, texel), GROUND_COLOUR, REVEALED_COVER_OPACITY);
}

// Excavation preview: a nonzero `pit` selects a pit; otherwise `shape` is a `VolumeShape::gpu_code`.
// `world` is in world pixels from the cell's bottom-left corner; `texel` is world pixels per
// screen pixel.
fn single_cell_excavation(pit: u32, shape: u32, world: vec2<f32>, texel: f32) -> vec3<f32> {
    let p = world / CELL_SIZE - vec2<f32>(0.5);
    if pit != 0u { return single_cell_open(0u, p, world, texel); }
    let ramp = shape_ramp(shape);
    if ramp != 0u { return single_cell_open(ramp, p, world, texel); }
    return single_cell_tunnel(shape_openings(shape), p, texel);
}
