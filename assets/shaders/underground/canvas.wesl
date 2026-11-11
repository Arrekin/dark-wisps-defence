// Renders underground terrain from packed obstacle cells on a quad covering the map.
//
// Pits and ramps draw their cliffs and rims inside their own cell, leaving neighbouring ground
// intact. Heights range from 0 at a pit floor to 1 at ground level. A higher neighbour creates a
// cliff along the shared edge or corner; overlapping cliffs use the highest surface. Cells outside
// the map have ground height, so excavations at the map boundary also have cliffs.
//
// The bottom pass draws floors, ramps, cliffs, shadows and tunnel interiors below underground
// wisps and floor covers. The top pass draws ground and overhanging rims above them. Ground covers
// tunnels and becomes translucent during tunnel reveal.
//
// A tunnel entrance requires an opening facing a pit or the foot of a ramp. Its arch is cut into
// that cell's cliff, leaving the ground above the tunnel intact. The cliff face and rim use the
// same mouth shape so their edges align.

import bevy_sprite_render::mesh2d::vertex_output::VertexOutput;
import package::shaders::common::core::CELL_SIZE;
import package::shaders::common::map_info::{grid_bounds, grid_contains};
import package::shaders::common::map_light::{MAP_SUN_ELEVATION_TANGENT, MAP_SUN_GROUND_DIRECTION};
import package::shaders::grids::obstacles::{STRUCTURE_EMPTY, STRUCTURE_EXCAVATION, STRUCTURE_MASK, obstacle_cell_at, underground_shape};
import package::shaders::underground::look::{
    ARCH_HEIGHT, FACE_WIDTH, FOOT_OCCLUSION, FOOT_OCCLUSION_REACH, GROUND_COLOUR, MOUTH_WIDTH, PASSAGE_FLOOR_TONE,
    PIT_DEPTH, RECESS_TONE, REVEALED_COVER_OPACITY, RIM_OVERHANG, RIM_WIDTH, SHADOW_SOFTNESS, SHADOW_STRENGTH, erosion_noise, face_colour,
    interior_colour, pit_floor_colour, ramp_colour, ramp_rise, rim_colour,
};
import package::shaders::walls::look::{passage_arm_distance, ramp_up};
import package::shaders::walls::styles::{opens_toward, shape_openings, shape_ramp};

// Field order and types mirror `UndergroundCanvasSettings` in
// map_objects_internal/src/underground_canvas.rs.
struct UndergroundCanvasSettings {
    canvas_pass: u32, // PASS_* value.
    tunnel_reveal: f32, // 0 keeps ground opaque; 1 uses REVEALED_COVER_OPACITY over tunnels.
}

@group(2) @binding(0) var<uniform> settings: UndergroundCanvasSettings;

const PASS_BOTTOM: u32 = 0u;
const PASS_TOP: u32 = 1u;

// Smallest height difference that forms a face.
const STEP_EPSILON: f32 = 0.02;
// Sentinel distance for searches with no candidate yet.
const FAR_DISTANCE: f32 = 1e6;

fn underground_kind(cell: u32) -> u32 {
    return cell & STRUCTURE_MASK;
}

fn cell_ramp(cell: u32) -> u32 {
    return shape_ramp(underground_shape(cell));
}

fn is_ramp(cell: u32) -> bool {
    return underground_kind(cell) == STRUCTURE_EXCAVATION && cell_ramp(cell) != 0u;
}

// Flat excavations count as tunnels here, including those with no openings.
fn is_tunnel(cell: u32) -> bool {
    return underground_kind(cell) == STRUCTURE_EXCAVATION && cell_ramp(cell) == 0u;
}

// Whether the cell is open to the sky: a pit or a ramp.
fn is_open(cell: u32) -> bool {
    return underground_kind(cell) == STRUCTURE_EMPTY || is_ramp(cell);
}

// Surface height from 0 (pit floor) to 1 (ground). `p` is in cell units from the cell centre.
fn height_in(cell: u32, p: vec2<f32>) -> f32 {
    if underground_kind(cell) == STRUCTURE_EMPTY { return 0.0; }
    if is_ramp(cell) { return ramp_rise(cell_ramp(cell), p); }
    return 1.0;
}

// A cliff surface sampled at a point inside a pit or ramp cell.
struct Cliff {
    // Surface height from 0 (pit floor) to 1 (ground); negative values lie below the floor.
    height: f32,
    // Distance from the cliff's rim, in world pixels.
    distance: f32,
    // Height difference at the rim, from 0 to 1; zero when no cliff was found.
    step: f32,
    // Unit normal in the map plane, pointing into the open cell.
    normal: vec2<f32>,
    // Grid offset to the higher neighbour, across an edge or corner.
    offset: vec2<i32>,
}

// Cardinal offset to the higher neighbour, or zero for a corner cliff.
fn cliff_side(cliff: Cliff) -> vec2<i32> {
    return select(vec2<i32>(0), cliff.offset, abs(cliff.offset.x) + abs(cliff.offset.y) == 1);
}

fn higher_cliff(a: Cliff, b: Cliff) -> Cliff {
    if b.height > a.height { return b; }
    return a;
}

// Finds the highest cliff at `p`, in cell units from the cell centre.
// All cliffs use the same slope, so taking the maximum joins their surfaces at intersections.
fn highest_cliff(coords: vec2<i32>, cell: u32, p: vec2<f32>) -> Cliff {
    var best = Cliff(-FAR_DISTANCE, FAR_DISTANCE, 0.0, vec2<f32>(0.0), vec2<i32>(0));

    var cardinals = array<vec2<i32>, 4>(vec2<i32>(1, 0), vec2<i32>(-1, 0), vec2<i32>(0, 1), vec2<i32>(0, -1));
    for (var i = 0; i < 4; i++) {
        let side = cardinals[i];
        let outward = vec2<f32>(side);
        let to_edge = 0.5 - dot(p, outward);
        let on_edge = p + outward * to_edge;
        let rim = height_in(obstacle_cell_at(coords + side), on_edge - outward);
        let step = rim - height_in(cell, on_edge);
        if step <= STEP_EPSILON { continue; }
        let distance = to_edge * CELL_SIZE;
        best = higher_cliff(best, Cliff(rim - distance / FACE_WIDTH, distance, step, -outward, side));
    }

    var diagonals = array<vec2<i32>, 4>(vec2<i32>(1, 1), vec2<i32>(-1, 1), vec2<i32>(1, -1), vec2<i32>(-1, -1));
    for (var i = 0; i < 4; i++) {
        let offset = diagonals[i];
        let corner = vec2<f32>(offset) * 0.5;
        let rim = height_in(obstacle_cell_at(coords + offset), corner - vec2<f32>(offset));
        let step = rim - height_in(cell, corner);
        if step <= STEP_EPSILON { continue; }
        let from_corner = p - corner;
        let distance = length(from_corner) * CELL_SIZE;
        best = higher_cliff(best, Cliff(rim - distance / FACE_WIDTH, distance, step, normalize(from_corner + vec2<f32>(1e-6)), offset));
    }
    return best;
}

// Whether the tunnel at `coords` opens toward `side` onto a pit or onto the foot of a ramp.
fn is_entrance(coords: vec2<i32>, side: vec2<i32>) -> bool {
    let cell = obstacle_cell_at(coords);
    if !is_tunnel(cell) || !opens_toward(shape_openings(underground_shape(cell)), side) { return false; }
    let across = coords + side;
    if !grid_contains(across) { return false; }
    let across_cell = obstacle_cell_at(across);
    return underground_kind(across_cell) == STRUCTURE_EMPTY || (is_ramp(across_cell) && all(ramp_up(cell_ramp(across_cell)) == side));
}

// Distance along a cardinal edge facing `outward` from its midpoint, in world pixels.
fn along_edge(p: vec2<f32>, outward: vec2<f32>) -> f32 {
    return dot(p, vec2<f32>(-outward.y, outward.x)) * CELL_SIZE;
}

// Surface colour at `p`, in cell units from the cell centre.
fn surface_colour(cell: u32, p: vec2<f32>) -> vec3<f32> {
    if underground_kind(cell) == STRUCTURE_EMPTY { return pit_floor_colour(); }
    if is_ramp(cell) {
        let ramp = cell_ramp(cell);
        return ramp_colour(pit_floor_colour(), ramp_rise(ramp, p), vec2<f32>(ramp_up(ramp)));
    }
    return GROUND_COLOUR;
}

// Approximate signed distance to a tunnel arch: negative inside, scaled by its smaller radius.
// Coordinates are in world pixels: `along` measures from the shared edge's midpoint;
// `into_open` measures from that edge into the pit or ramp. The arch lies entirely in its cliff.
fn mouth_distance(along: f32, into_open: f32) -> f32 {
    let radii = vec2<f32>(MOUTH_WIDTH * 0.5, FACE_WIDTH * ARCH_HEIGHT);
    let local = vec2<f32>(along, FACE_WIDTH - into_open);
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
fn open_surface(coords: vec2<i32>, cell: u32, p: vec2<f32>, texel: f32) -> vec3<f32> {
    let height = height_in(cell, p);
    let floor = pit_floor_colour();
    var colour = floor;
    if is_ramp(cell) {
        colour = ramp_colour(floor, height, vec2<f32>(ramp_up(cell_ramp(cell))));
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
    if any(side != vec2<i32>(0)) && is_entrance(coords + side, -side) {
        let opening = 1.0 - smoothstep(-texel, texel, mouth_distance(along_edge(p, vec2<f32>(side)), cliff.distance));
        face = mix(face, floor * PASSAGE_FLOOR_TONE * RECESS_TONE, opening);
    }
    return mix(face, colour, smoothstep(-texel, texel, beyond_foot));
}

// A tunnel's interior: earth around a passage from the cell centre to each open side.
fn tunnel_interior(cell: u32, p: vec2<f32>, texel: f32) -> vec3<f32> {
    let openings = shape_openings(underground_shape(cell));
    var sides = array<vec2<i32>, 4>(vec2<i32>(0, 1), vec2<i32>(1, 0), vec2<i32>(0, -1), vec2<i32>(-1, 0));
    var passage = FAR_DISTANCE;
    for (var i = 0; i < 4; i++) {
        if opens_toward(openings, sides[i]) {
            passage = min(passage, passage_arm_distance(p, vec2<f32>(sides[i])));
        }
    }
    return interior_colour(passage, texel);
}

// Ground opacity, reduced over tunnels according to the reveal amount.
fn reveal_opacity(cell: u32) -> f32 {
    return select(1.0, mix(1.0, REVEALED_COVER_OPACITY, settings.tunnel_reveal), is_tunnel(cell));
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
    let open = is_open(cell);

    if settings.canvas_pass == PASS_BOTTOM {
        if open { return vec4<f32>(open_surface(coords, cell, p, texel), 1.0); }
        // Opaque ground hides a tunnel's interior until the reveal begins.
        if is_tunnel(cell) && settings.tunnel_reveal > 0.0 {
            return vec4<f32>(tunnel_interior(cell, p, texel), 1.0);
        }
        return vec4<f32>(0.0);
    }

    if !open { return vec4<f32>(GROUND_COLOUR, reveal_opacity(cell)); }

    // Skip pixels beyond the maximum rim width before searching for a cliff.
    let from_edges = (0.5 - max(abs(p.x), abs(p.y))) * CELL_SIZE;
    if from_edges > RIM_WIDTH + RIM_OVERHANG + texel { return vec4<f32>(0.0); }
    let cliff = highest_cliff(coords, cell, p);
    if cliff.step <= 0.0 { return vec4<f32>(0.0); }
    let overhang = (RIM_WIDTH + RIM_OVERHANG * erosion_noise(world)) * cliff.step;
    var lip_edge = overhang - cliff.distance;
    let side = cliff_side(cliff);
    if any(side != vec2<i32>(0)) && is_entrance(coords + side, -side) {
        // Clip the overhang to the arch so the rim does not cover the tunnel mouth.
        lip_edge = min(lip_edge, mouth_distance(along_edge(p, vec2<f32>(side)), cliff.distance));
    }
    let coverage = smoothstep(-texel, texel, lip_edge);
    if coverage <= 0.0 { return vec4<f32>(0.0); }
    let above = surface_colour(obstacle_cell_at(coords + cliff.offset), p - vec2<f32>(cliff.offset));
    return vec4<f32>(rim_colour(above, cliff.normal, max(lip_edge, 0.0)), coverage);
}
