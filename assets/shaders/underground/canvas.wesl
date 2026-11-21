// Renders underground terrain from packed obstacle cells on a quad covering the map.
//
// Pits and ramps draw their cliffs and rims inside their own cell, leaving neighbouring ground
// intact. Heights range from 0 at a pit floor to 1 at ground level. A higher neighbour creates a
// cliff along the shared edge or corner; overlapping cliffs use the highest surface. Cells outside
// the map have ground height, so excavations at the map boundary also have cliffs.
//
// The bottom pass draws floors, ramps, cliffs, shadows, and hollow excavation interiors below
// underground wisps and floor covers. The top pass draws ground and overhanging rims above them.
// Ground over flat excavations becomes translucent during tunnel reveal.
//
// Where a hollow excavation opens onto a pit or ramp, an entrance arch is cut into the adjacent
// cliff while keeping the ground above the excavation intact. The cliff face and rim share the
// same mouth SDF so their edges align.

import bevy_sprite_render::mesh2d::vertex_output::VertexOutput;
import package::shaders::common::core::CELL_SIZE;
import package::shaders::common::map_info::grid_bounds;
import package::shaders::common::map_light::{MAP_SUN_ELEVATION_TANGENT, MAP_SUN_GROUND_DIRECTION};
import package::shaders::grids::obstacles::{LEVEL_UNDERGROUND, STRUCTURE_EMPTY, STRUCTURE_EXCAVATION, ObstacleCell, cell_structure_shape, obstacle_cell_at};
import package::shaders::grids::openings::is_entrance;
import package::shaders::grids::structure_shape::{passage_distance, ramp_up, shape_hollowed_sides};
import package::shaders::underground::look::{
    ARCH_HEIGHT, FACE_WIDTH, FOOT_OCCLUSION, FOOT_OCCLUSION_REACH, GROUND_COLOUR, MOUTH_WIDTH, PASSAGE_FLOOR_TONE,
    PIT_DEPTH, RECESS_TONE, REVEALED_COVER_OPACITY, RIM_OVERHANG, RIM_WIDTH, SHADOW_SOFTNESS, SHADOW_STRENGTH, erosion_noise, face_colour,
    interior_colour, pit_floor_colour, ramp_colour, ramp_rise, rim_colour,
};
import package::shaders::underground::surface::{STEP_EPSILON, Cliff, height_in, highest_cliff, is_ramp, underground_kind, underground_ramp};

// Field order and types mirror `UndergroundCanvasSettings` in
// map_objects_internal/src/underground_canvas.rs.
struct UndergroundCanvasSettings {
    canvas_pass: u32, // PASS_* value.
    tunnel_reveal: f32, // 0 keeps ground opaque; 1 uses REVEALED_COVER_OPACITY over excavations.
}

@group(2) @binding(0) var<uniform> settings: UndergroundCanvasSettings;

const PASS_BOTTOM: u32 = 0u;
const PASS_TOP: u32 = 1u;

// True if the cell holds a flat excavation (preserving the ground floor above it).
// Mirrors `ExcavationShape::keeps_ground_floor` in `map_objects/src/lib.rs`.
fn keeps_ground_floor(cell: ObstacleCell) -> bool {
    return underground_kind(cell) == STRUCTURE_EXCAVATION && underground_ramp(cell) == 0u;
}

// Whether the cell holds an excavation open to the air above it: a pit or a ramp.
fn is_open_air_excavation(cell: ObstacleCell) -> bool {
    return underground_kind(cell) == STRUCTURE_EMPTY || is_ramp(cell);
}

// Cardinal offset to the higher neighbour, or zero for a corner cliff.
fn cliff_side(cliff: Cliff) -> vec2<i32> {
    return select(vec2<i32>(0), cliff.offset, abs(cliff.offset.x) + abs(cliff.offset.y) == 1);
}

// Distance along a cardinal edge facing `outward` from its midpoint, in world pixels.
fn along_edge(p: vec2<f32>, outward: vec2<f32>) -> f32 {
    return dot(p, vec2<f32>(-outward.y, outward.x)) * CELL_SIZE;
}

// Surface colour at `p`, in cell units from the cell centre.
fn surface_colour(cell: ObstacleCell, p: vec2<f32>) -> vec3<f32> {
    if underground_kind(cell) == STRUCTURE_EMPTY { return pit_floor_colour(); }
    if is_ramp(cell) {
        let ramp = underground_ramp(cell);
        return ramp_colour(pit_floor_colour(), ramp_rise(ramp, p), vec2<f32>(ramp_up(ramp)));
    }
    return GROUND_COLOUR;
}

// Approximate signed distance to an entrance's arch: negative inside, scaled by its smaller radius.
// Coordinates are in world pixels: `along` measures from the shared edge's midpoint;
// `into_pit_or_ramp` measures from that edge into the pit or ramp. The arch lies entirely in its cliff.
fn mouth_distance(along: f32, into_pit_or_ramp: f32) -> f32 {
    let radii = vec2<f32>(MOUTH_WIDTH * 0.5, FACE_WIDTH * ARCH_HEIGHT);
    let local = vec2<f32>(along, FACE_WIDTH - into_pit_or_ramp);
    return (length(local / radii) - 1.0) * min(radii.x, radii.y);
}

// Shadow strength at `p`, in cell units from the cell centre, at normalized surface `height`.
// With the configured pit depth and sun angle, only the cardinal and diagonal neighbours toward
// the sun can cast shadows here. Sample each neighbour's height where the ray enters it.
fn sun_shadow(coords: vec2<i32>, p: vec2<f32>, height: f32) -> f32 {
    let reach = PIT_DEPTH / MAP_SUN_ELEVATION_TANGENT;
    let toward = vec2<i32>(sign(MAP_SUN_GROUND_DIRECTION));
    let local = p * CELL_SIZE;
    let ray_end = local + MAP_SUN_GROUND_DIRECTION * reach;
    if all(abs(ray_end) < vec2<f32>(CELL_SIZE * 0.5)) { return 0.0; }
    // Ray length to each of the cell's two edges facing the sun.
    let to_edges = (vec2<f32>(CELL_SIZE * 0.5) - local * vec2<f32>(toward)) / abs(MAP_SUN_GROUND_DIRECTION);
    let first = select(vec2<i32>(0, toward.y), vec2<i32>(toward.x, 0), to_edges.x < to_edges.y);
    var entries = array<f32, 2>(min(to_edges.x, to_edges.y), max(to_edges.x, to_edges.y));
    var offsets = array<vec2<i32>, 2>(first, toward);
    var shadow = 0.0;
    for (var i = 0; i < 2; i++) {
        let distance = entries[i];
        if distance >= reach { break; }
        let entry = (local + MAP_SUN_GROUND_DIRECTION * distance) / CELL_SIZE - vec2<f32>(offsets[i]);
        let rim = height_in(obstacle_cell_at(coords + offsets[i]), entry);
        // Only a neighbour standing higher than the surface casts a shadow.
        if rim - height <= STEP_EPSILON { continue; }
        let above = (rim - height) * PIT_DEPTH - distance * MAP_SUN_ELEVATION_TANGENT;
        shadow = max(shadow, smoothstep(-SHADOW_SOFTNESS, SHADOW_SOFTNESS, above));
    }
    return shadow;
}

// A pit floor or ramp, with its cliffs and the shadow its rims cast on it.
fn pit_or_ramp_surface(coords: vec2<i32>, cell: ObstacleCell, p: vec2<f32>, texel: f32) -> vec3<f32> {
    let height = height_in(cell, p);
    let floor = pit_floor_colour();
    var colour = floor;
    if is_ramp(cell) {
        colour = ramp_colour(floor, height, vec2<f32>(ramp_up(underground_ramp(cell))));
    }

    // Apply cast shadows to the floor or ramp; cliff lighting uses the face normal.
    colour *= 1.0 - SHADOW_STRENGTH * sun_shadow(coords, p, height);

    let cliff = highest_cliff(coords, cell, p);
    if cliff.step <= 0.0 { return colour; }
    // Signed distance from the cliff's foot in world pixels, positive on the cell's own surface.
    let beyond_foot = (height - cliff.height) * FACE_WIDTH;
    let falloff = saturate(1.0 - max(beyond_foot, 0.0) / FOOT_OCCLUSION_REACH);
    colour *= 1.0 - FOOT_OCCLUSION * cliff.step * falloff;
    if beyond_foot >= texel { return colour; }

    let depth = saturate(cliff.distance / max(FACE_WIDTH * cliff.step, texel));
    var face = face_colour(cliff.normal, depth);
    let side = cliff_side(cliff);
    if any(side != vec2<i32>(0)) && is_entrance(coords + side, -side, LEVEL_UNDERGROUND) {
        let mouth = 1.0 - smoothstep(-texel, texel, mouth_distance(along_edge(p, vec2<f32>(side)), cliff.distance));
        face = mix(face, floor * PASSAGE_FLOOR_TONE * RECESS_TONE, mouth);
    }
    return mix(face, colour, smoothstep(-texel, texel, beyond_foot));
}

// Shades the underground interior of a flat excavation around its carved passage.
fn interior(cell: ObstacleCell, p: vec2<f32>, texel: f32) -> vec3<f32> {
    return interior_colour(passage_distance(shape_hollowed_sides(cell_structure_shape(cell, LEVEL_UNDERGROUND)), p), texel);
}

// Ground alpha, faded over flat excavations according to `settings.tunnel_reveal`.
fn reveal_opacity(cell: ObstacleCell) -> f32 {
    return select(1.0, mix(1.0, REVEALED_COVER_OPACITY, settings.tunnel_reveal), keeps_ground_floor(cell));
}

@fragment
fn fragment(mesh: VertexOutput) -> @location(0) vec4<f32> {
    let f = mesh.uv * vec2<f32>(grid_bounds());
    let base = floor(f);
    let coords = vec2<i32>(base);
    let world = f * CELL_SIZE;
    let p = f - base - vec2<f32>(0.5);
    // World-coordinate span of one screen pixel; evaluated before divergent control flow.
    let texel = max(max(fwidth(world.x), fwidth(world.y)), 0.0001);

    let cell = obstacle_cell_at(coords);
    let open_air_excavation = is_open_air_excavation(cell);

    if settings.canvas_pass == PASS_BOTTOM {
        if open_air_excavation { return vec4<f32>(pit_or_ramp_surface(coords, cell, p, texel), 1.0); }
        // Opaque ground hides the interior until the reveal begins.
        if keeps_ground_floor(cell) && settings.tunnel_reveal > 0.0 {
            return vec4<f32>(interior(cell, p, texel), 1.0);
        }
        return vec4<f32>(0.0);
    }

    if !open_air_excavation { return vec4<f32>(GROUND_COLOUR, reveal_opacity(cell)); }

    // Skip pixels beyond the maximum rim width before searching for a cliff.
    let from_edges = (0.5 - max(abs(p.x), abs(p.y))) * CELL_SIZE;
    if from_edges > RIM_WIDTH + RIM_OVERHANG + texel { return vec4<f32>(0.0); }
    let cliff = highest_cliff(coords, cell, p);
    if cliff.step <= 0.0 { return vec4<f32>(0.0); }
    let overhang = (RIM_WIDTH + RIM_OVERHANG * erosion_noise(world)) * cliff.step;
    var lip_edge = overhang - cliff.distance;
    let side = cliff_side(cliff);
    if any(side != vec2<i32>(0)) && is_entrance(coords + side, -side, LEVEL_UNDERGROUND) {
        // Clip the overhang to the arch so the rim does not cover the mouth.
        lip_edge = min(lip_edge, mouth_distance(along_edge(p, vec2<f32>(side)), cliff.distance));
    }
    let coverage = smoothstep(-texel, texel, lip_edge);
    if coverage <= 0.0 { return vec4<f32>(0.0); }
    let above = surface_colour(obstacle_cell_at(coords + cliff.offset), p - vec2<f32>(cliff.offset));
    return vec4<f32>(rim_colour(above, cliff.normal, max(lip_edge, 0.0)), coverage);
}
