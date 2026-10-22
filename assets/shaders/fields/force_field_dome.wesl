// Force field dome: a transparent hemisphere seen from above. `displace` distorts the scene
// beneath it; `overlay` adds light. The shared field index clips both passes to the field's
// territory, except for rim light extending onto unclaimed ground.
//
// Surface patterns compress toward the rim to suggest curvature:
// - strands trace the territory's edge with thin, shimmering lines;
// - veils form pale streaks flowing around the outer shell;
// - moving cell boundaries distort and shade the scene like bevelled glass;
// - lens distortion and slow wobble bend the scene near the rim;
// - a faint film marks seams between domes;
// - wisp impacts send ripples across the shell.

import bevy_sprite_render::mesh2d::{vertex_output::VertexOutput, view_bindings::globals};
import package::shaders::common::color::display_to_linear;
import package::shaders::common::core::{PI, TAU};
import package::shaders::common::hash::{HASH_GOLDEN, hash_mix, hash_unit};
import package::shaders::common::displacement::world_offset_to_uv;
import package::shaders::common::map_light::MAP_SUN_GROUND_DIRECTION;
import package::shaders::common::value_noise::{value_fbm_3d, value_noise_3d};
import package::shaders::fields::fields::{FieldBoundary, FieldShape, NO_FIELD, field_boundary, field_owners_at, field_shapes};

// Mirrors `MAX_FIELD_IMPACTS` in fields_internal/src/force_field.rs.
const MAX_FIELD_IMPACTS: u32 = 8u;

// Field order and types mirror `ForceFieldDome` in fields_internal/src/force_field.rs.
struct Instance {
    radius: f32,
    progress: f32,
    seed: f32,
    impact_count: u32,
    // Index slot containing the field's centre and current radius.
    slot: u32,
    // Wisp entries into the dome: `xy` is the hit position, `z` the age in seconds.
    impacts: array<vec4<f32>, MAX_FIELD_IMPACTS>,
};

// Colours, in sRGB
const ICE_BLUE: vec3<f32> = vec3<f32>(0.16, 0.776, 1.0);
const ULTRAVIOLET: vec3<f32> = vec3<f32>(0.45, 0.34, 1.0);

// Colours, in linear light
const PALE: vec3<f32> = vec3<f32>(0.9, 0.95, 1.0);
const NEUTRAL: vec3<f32> = vec3<f32>(0.82, 0.9, 1.0);

// Rim strands
const STRAND_COUNT: i32 = 4;
const STRAND_BRIGHTNESS: f32 = 1.0;
const STRAND_SPACING: f32 = 5.0;          // px between strands, from the edge inward
const STRAND_WIDTH: f32 = 0.8;            // px
const STRAND_WANDER: f32 = 4.0;           // px each strand drifts in and out
const STRAND_SPEED: f32 = 0.15;
const STRAND_DISPERSION: f32 = 0.7;       // px between colour channels: a fringe around a white core
const STRAND_VARY: f32 = 0.75;            // how much brightness varies around the rim
const STRAND_TINT: f32 = 0.5;             // 0 white core, 1 fully ice blue
const STRAND_GLOW: f32 = 0.2;             // glow band just inside the rim
const STRAND_GLOW_WIDTH: f32 = 12.0;      // px
const STRAND_SEAM: f32 = 0.35;            // seam brightness relative to rim brightness
const STRAND_REACH: f32 = 80.0;           // px from the edge beyond which the strands and their glow are dark
// Maximum rim-light extension onto unclaimed ground, in px. Allows for strand motion,
// shimmer, colour fringes, and glow. Mirrors `OUTER_REACH` in fields_internal/src/force_field.rs.
const OUTER_REACH: f32 = 10.0;

// Shimmer on the strands
const SHIMMER_SHIFT: f32 = 1.5;           // px the strands quiver
const SHIMMER_GRAIN: f32 = 0.4;           // strength of brightness noise along strands
const SHIMMER_SCALE: f32 = 16.0;
const SHIMMER_SPEED: f32 = 1.0;
const SHIMMER_CENTRE: f32 = 0.4;          // centre strength relative to rim strength

// Veils
const VEIL_BRIGHTNESS: f32 = 1.0;
const VEIL_REACH: f32 = 0.45;             // band width as a fraction of the radius, measured inward
const VEIL_ALONG: f32 = 1.4;              // pattern frequency around the rim: lower makes longer streaks
const VEIL_ACROSS: f32 = 18.0;            // pattern frequency across the band
const VEIL_WARP: f32 = 0.6;
const VEIL_CONTRAST: f32 = 0.7;
const VEIL_FLOW: f32 = 0.04;              // rad/s around the dome
const VEIL_ACCENT: f32 = 0.25;            // share of ultraviolet in the colour
const VEIL_PALENESS: f32 = 0.1;

// Web of cuts
const WEB_CELLS: u32 = 40u;               // cells of the main web over the whole sphere
const FINE_WEB_CELLS: u32 = 100u;         // cells of the finer web
const WEB_FLOW: f32 = 0.06;               // mean cell speed over the shell, rad/s
const FINE_WEB_SPEED: f32 = 1.3;          // speed multiplier for the finer web
const WEB_SALT: u32 = 0x5be0cd19u;
const FINE_WEB_SALT: u32 = 0x1f83d9abu;
const WEB_SHIFT: f32 = 5.0;               // px the scene shifts on each side of a cut
const WEB_BEVEL: f32 = 8.0;               // px over which the shift fades away from a cut
const WEB_BURN: f32 = 0.5;                // bevel brightness change relative to the scene beneath
const WEB_CREASE: f32 = 0.25;             // dark crease along a cut
const WEB_KINK: f32 = 0.5;                // how much the strands and veils shift with the cuts
const WEB_FINE: f32 = 0.3;                // finer web strength relative to the main web
const WEB_WARP: f32 = 0.5;                // curving of the cuts
const WEB_EDGE_ON: f32 = 0.7;             // cuts stronger where the shell is seen edge-on
const WEB_CENTRE: f32 = 0.5;              // centre strength relative to rim strength

// Lens and wobble
const LENS: f32 = 4.0;                    // px the scene bends toward the rim
const WOBBLE: f32 = 1.5;                  // px of slow wobble in the veil band

// Seams
const WALL_HEIGHT_POWER: f32 = 0.6;       // how fast the seam wall lowers toward the rims
const FILM_WIDTH: f32 = 4.0;              // px
const FILM_OPACITY: f32 = 0.55;
const FILM_TINT: f32 = 0.6;               // neutral toward ice blue

// Impact ripples
const RIPPLE_SPEED: f32 = 60.0;           // px/s the wavefront expands
// Distance multiplier along the rim-to-centre axis. Slows radial propagation to suggest
// travel over the dome; π/2 is the ratio of a semicircle's arc length to its diameter.
const DOME_WARP: f32 = 1.57;
// Fraction of the current radius where a wave starts: 1 is the rim, lower is farther up the dome.
const RIPPLE_INSET: f32 = 0.91;
const RIPPLE_DECAY: f32 = 0.7;            // exponential time-based envelope falloff
const RIPPLE_PULSE_SHARPNESS: f32 = 0.15; // Gaussian sharpness, per px² of phase; larger = narrower ring
const RIPPLE_STRENGTH: f32 = 6.0;         // peak refraction displacement, px
const RIPPLE_GLOW: f32 = 0.5;             // peak additive brightness at the wavefront
const RIPPLE_COLOR: vec3<f32> = vec3<f32>(0.5, 0.9, 1.0);

// ── Shell ────────────────────────────────────────────────────────────────────

// Dome geometry above a ground point, with height and normal computed on a unit hemisphere.
struct Shell {
    // From the dome's centre to the point, in px.
    offset: vec2<f32>,
    radius: f32,
    // Distance from the centre divided by the radius.
    reach: f32,
    // Height of the shell there: 1 at the apex, 0 at the rim.
    height: f32,
    normal: vec3<f32>,
    // Angle around the dome.
    around: f32,
    // Unit vector from the dome's centre toward the point.
    direction: vec2<f32>,
};

fn shell_at(world_position: vec2<f32>, center: vec2<f32>, radius: f32) -> Shell {
    let offset = world_position - center;
    let reach = clamp(length(offset) / radius, 0.0, 0.999);
    let height = sqrt(1.0 - reach * reach);
    let direction = select(vec2<f32>(0.0, 1.0), offset / length(offset), reach > 0.0005);
    return Shell(offset, radius, reach, height, vec3<f32>(offset / radius, height), atan2(offset.y, offset.x), direction);
}

// Noise around the circle with no seam where the angle wraps; `frequency` is features per radian, roughly.
fn ring_noise(around: f32, frequency: f32, z: f32) -> f32 {
    return value_noise_3d(vec3<f32>(cos(around) * frequency, sin(around) * frequency, z));
}

// Strength of a shell pattern between the centre and the rim.
fn toward_rim(shell: Shell, centre: f32) -> f32 {
    return mix(centre, 1.0, shell.reach * shell.reach);
}

// ── Rim strands ──────────────────────────────────────────────────────────────

// Draws strands at `edge` pixels inside the boundary (negative outside).
// Offsetting the colour channels creates fringes around a white core.
fn rim_strands(around: f32, edge: f32, time: f32, seed: f32) -> vec3<f32> {
    let vary = mix(1.0, 0.25 + 1.5 * ring_noise(around, 1.2, time * 0.08 + seed), STRAND_VARY);
    let offsets = vec3<f32>(STRAND_DISPERSION, -STRAND_DISPERSION, 0.0);
    let widths = vec3<f32>(STRAND_WIDTH, STRAND_WIDTH, STRAND_WIDTH * 1.6);
    var lines = vec3<f32>(0.0);
    for (var k = 0; k < STRAND_COUNT; k++) {
        let index = f32(k);
        // The outermost strand hugs the edge; inner ones wander more.
        let freedom = 0.3 + 0.7 * index / f32(STRAND_COUNT - 1);
        let wander = (ring_noise(around, 3.0 + index, time * STRAND_SPEED + seed + index * 7.1) - 0.5) * 2.0 * STRAND_WANDER * freedom;
        let at = index * STRAND_SPACING + wander;
        let strength = (1.0 - index / f32(STRAND_COUNT + 1)) * (0.35 + 0.65 * ring_noise(around, 6.0, time * 0.3 + index * 3.3 + seed));
        let distances = vec3<f32>(edge - at) + offsets;
        lines += exp(-(distances * distances) / (widths * widths)) * strength;
    }
    let ice = display_to_linear(ICE_BLUE);
    let glow = ice * exp(-max(edge, 0.0) / STRAND_GLOW_WIDTH) * step(0.0, edge) * STRAND_GLOW;
    let tint = mix(vec3<f32>(1.0), ice / max(max(ice.r, ice.g), ice.b), STRAND_TINT);
    return (lines * tint * STRAND_BRIGHTNESS + glow) * vary;
}

// Adds rapid position and brightness noise to the strands for a shimmering effect.
fn shimmering_strands(shell: Shell, around: f32, edge: f32, time: f32, seed: f32) -> vec3<f32> {
    let coarse = shell.normal * SHIMMER_SCALE + vec3<f32>(seed, seed * 0.7, time * SHIMMER_SPEED);
    let fine = shell.normal * SHIMMER_SCALE * 2.3 + vec3<f32>(3.1, seed, -time * SHIMMER_SPEED * 1.4);
    let weight = toward_rim(shell, SHIMMER_CENTRE);
    let shift = ((vec2<f32>(value_noise_3d(coarse), value_noise_3d(coarse + 17.3)) - 0.5) * 1.4
               + (vec2<f32>(value_noise_3d(fine), value_noise_3d(fine + 9.1)) - 0.5) * 0.6) * weight * 2.0 * SHIMMER_SHIFT;
    let grain = mix(1.0, mix(1.0, 0.35 + 1.3 * value_noise_3d(fine * 1.3 + 5.0), SHIMMER_GRAIN), weight);
    return rim_strands(around + shift.y / shell.radius, edge + shift.x, time, seed) * grain;
}

// ── Veils ────────────────────────────────────────────────────────────────────

// Draws pale streaks along the outer shell. Angular coordinates stretch them around the dome
// and compress their spacing toward the rim.
fn veils(shell: Shell, around: f32, time: f32, seed: f32) -> vec3<f32> {
    let depth = acos(shell.height) / (PI * 0.5);
    var mask = smoothstep(1.0 - VEIL_REACH, 1.0, shell.reach);
    mask *= mask;
    let flow = around + time * VEIL_FLOW;
    let warp = (value_noise_3d(vec3<f32>(cos(flow) * 2.0, sin(flow) * 2.0, depth * 3.0 + time * 0.05 + seed)) - 0.5) * VEIL_WARP;
    let across = depth * VEIL_ACROSS + warp * 4.0;
    let pattern = value_fbm_3d(vec3<f32>(cos(flow) * VEIL_ALONG, sin(flow) * VEIL_ALONG, across + seed), 4);
    let softness = (1.0 - VEIL_CONTRAST) * 0.5;
    let sheet = smoothstep(0.5 - softness, 0.5 + softness + 0.02, pattern);
    let hue = ring_noise(around - time * VEIL_FLOW * 0.5, 1.3, time * 0.03 + seed + 11.0);
    var colour = mix(display_to_linear(ICE_BLUE), display_to_linear(ULTRAVIOLET), smoothstep(0.35, 0.75, hue) * VEIL_ACCENT);
    colour = mix(colour, PALE, VEIL_PALENESS);
    let edge_on = min(1.0 / max(shell.height, 0.2), 3.0);
    return colour * sheet * mask * edge_on * VEIL_BRIGHTNESS * 0.25;
}

// ── Web of cuts ──────────────────────────────────────────────────────────────

fn random_unit_vector(hash: u32) -> vec3<f32> {
    let height = hash_unit(hash) * 2.0 - 1.0;
    let around = hash_unit(hash_mix(hash ^ HASH_GOLDEN)) * TAU;
    let across = sqrt(1.0 - height * height);
    return vec3<f32>(across * cos(around), across * sin(around), height);
}

// Rotates `v` by `angle` radians about a unit axis.
fn rotate_about(v: vec3<f32>, axis: vec3<f32>, angle: f32) -> vec3<f32> {
    return v * cos(angle) + cross(axis, v) * sin(angle) + axis * dot(axis, v) * (1.0 - cos(angle));
}

// Cell centre on the unit sphere, moving along a great circle.
// `index` and `salt` determine its starting point, rotation axis, direction, and speed.
fn web_cell(index: u32, salt: u32, time: f32) -> vec3<f32> {
    let hash = hash_mix((index * 977u + 13u) ^ salt);
    let start = random_unit_vector(hash);
    let axis = normalize(cross(start, random_unit_vector(hash_mix(hash + 7u))));
    let direction = select(1.0, -1.0, hash_unit(hash_mix(hash + 9u)) < 0.5);
    let speed = WEB_FLOW * (0.4 + 1.2 * hash_unit(hash_mix(hash + 8u))) * direction;
    return rotate_about(start, axis, time * speed);
}

// Estimated distance to a web cut in pixels, and the direction away from it into the current cell.
struct WebCut {
    distance: f32,
    away: vec2<f32>,
};

// Approximates the nearest cut using the two closest cell centres. Their dividing plane passes
// through the sphere's centre. The projected gradient converts distance to that plane into pixels.
fn web_cut(shell: Shell, point: vec3<f32>, count: u32, salt: u32, time: f32) -> WebCut {
    var nearest = -2.0;
    var nearest_cell = vec3<f32>(0.0, 0.0, 1.0);
    var second = -2.0;
    var second_cell = vec3<f32>(0.0, 0.0, -1.0);
    for (var k = 0u; k < count; k++) {
        let cell = web_cell(k, salt, time);
        let closeness = dot(point, cell);
        if closeness > nearest {
            second = nearest;
            second_cell = nearest_cell;
            nearest = closeness;
            nearest_cell = cell;
        } else if closeness > second {
            second = closeness;
            second_cell = cell;
        }
    }
    let wall = normalize(nearest_cell - second_cell);
    let edge = dot(point, wall);
    // Screen-space gradient of the distance to the dividing plane.
    let gradient = wall.xy / shell.radius - wall.z * shell.offset / (shell.radius * shell.radius * max(shell.height, 0.05));
    let rate = max(length(gradient), 1.0e-6);
    return WebCut(edge / rate, gradient / rate);
}

// The scene shift (xy, px) and brightness change (z) of both webs.
fn web_cuts(shell: Shell, time: f32, seed: f32) -> vec3<f32> {
    let n = shell.normal;
    let warp = vec3<f32>(
        value_noise_3d(n * 2.5 + vec3<f32>(seed, 0.0, time * 0.05)),
        value_noise_3d(n * 2.5 + vec3<f32>(5.2, seed, time * 0.05)),
        value_noise_3d(n * 2.5 + vec3<f32>(9.7, 3.1, seed + time * 0.05)),
    ) - 0.5;
    let point = normalize(n + warp * WEB_WARP * 0.25);
    let dome_salt = bitcast<u32>(seed);
    var total = vec3<f32>(0.0);
    for (var layer = 0u; layer < 2u; layer++) {
        let main = layer == 0u;
        let weight = select(WEB_FINE, 1.0, main);
        let count = select(FINE_WEB_CELLS, WEB_CELLS, main);
        let salt = dome_salt ^ select(FINE_WEB_SALT, WEB_SALT, main);
        let cut = web_cut(shell, point, count, salt, time * select(FINE_WEB_SPEED, 1.0, main));
        let bevel = exp(-cut.distance / WEB_BEVEL);
        let crease = exp(-(cut.distance * cut.distance) / 0.8);
        total += vec3<f32>(
            cut.away * WEB_SHIFT * bevel,
            dot(cut.away, MAP_SUN_GROUND_DIRECTION) * WEB_BURN * bevel - crease * WEB_CREASE,
        ) * weight;
    }
    return total * toward_rim(shell, WEB_CENTRE);
}

// ── Impact ripples ───────────────────────────────────────────────────────────

// Returns ripple displacement in pixels (xy) and glow (z) from wisp impacts.
// A Gaussian envelope concentrates each wave around its expanding front.
fn impact_ripples(world_position: vec2<f32>, shell: Shell, field: Instance, center: vec2<f32>) -> vec3<f32> {
    // The shell's thickness along the line of sight: the glow is brighter toward the rim.
    let dome = clamp((1.0 / shell.height - 1.0) * 0.5, 0.0, 1.0) * smoothstep(1.0, 0.82, shell.reach);
    var displacement = vec2<f32>(0.0);
    var glow = 0.0;
    for (var index = 0u; index < field.impact_count; index++) {
        let impact = field.impacts[index];
        let hit_position = impact.xy;
        let age = impact.z;

        // A wave starts a little inside the rim, so the impact hits the dome above the ground.
        let hit_to_center = normalize(center - hit_position);
        let inset_radius = shell.radius * RIPPLE_INSET;
        let origin = center - hit_to_center * inset_radius;

        // Stretch radial distance so the ring expands more slowly toward the dome's centre.
        let delta = world_position - origin;
        let along = dot(delta, hit_to_center);
        let across = length(delta - along * hit_to_center);
        let dome_distance = length(vec2<f32>(along * DOME_WARP, across));

        // Delay expansion by the travel time from the hit position to the inset origin.
        let hit_distance = length(hit_position - center);
        let travel_time = max(0.0, hit_distance - inset_radius) / RIPPLE_SPEED;
        let expand_age = max(0.0, age - travel_time);
        let phase = dome_distance - expand_age * RIPPLE_SPEED;

        let envelope = exp(-age * RIPPLE_DECAY) * exp(-phase * phase * RIPPLE_PULSE_SHARPNESS);
        // cos peaks at the wavefront, so the displacement ring sits on the expanding front.
        let value = cos(phase * 1.5) * envelope;
        let push = select(shell.direction, normalize(delta), length(delta) > 1.0);
        displacement += push * value;
        glow += max(0.0, value) * dome;
    }
    return vec3<f32>(displacement * RIPPLE_STRENGTH, glow * RIPPLE_GLOW);
}

// ── Dome ─────────────────────────────────────────────────────────────────────

// Geometry and effects shared by the displacement and light passes.
struct Surface {
    shell: Shell,
    boundary: FieldBoundary,
    // The scene shift of the cuts and ripples, in px.
    shift: vec2<f32>,
    // Brightness change from the cuts, relative to the scene beneath.
    burn: f32,
    ripple_glow: f32,
};

// With `for_light`, skips web calculations where neither strands nor veils are visible.
fn surface_at(world_position: vec2<f32>, field: Instance, shape: FieldShape, time: f32, for_light: bool) -> Surface {
    let shell = shell_at(world_position, shape.center, shape.current_radius);
    let boundary = field_boundary(field.slot, world_position);
    let dark = for_light && shell.reach < 1.0 - VEIL_REACH && boundary.edge > STRAND_REACH;
    var cuts = vec3<f32>(0.0);
    if !dark {
        cuts = web_cuts(shell, time, field.seed);
    }
    let edge_on = mix(1.0, 0.35 + 1.6 * pow(1.0 - shell.height, 1.5), WEB_EDGE_ON);
    let ripples = impact_ripples(world_position, shell, field, shape.center);
    // Displacement fades in from the edge so the scene does not tear along it.
    let shift = (cuts.xy * edge_on - ripples.xy) * smoothstep(0.0, 4.0, boundary.edge);
    let burn = cuts.z * edge_on * smoothstep(0.0, 2.0, boundary.edge);
    return Surface(shell, boundary, shift, burn, ripples.z);
}

fn displace(in: VertexOutput, field: Instance) -> vec4<f32> {
    let world_position = in.world_position.xy;
    let shape = field_shapes[field.slot];
    if shape.current_radius <= 0.0 || field_owners_at(world_position).first.slot != field.slot {
        return vec4<f32>(0.0);
    }
    let surface = surface_at(world_position, field, shape, globals.time, false);
    let shell = surface.shell;
    let band = smoothstep(1.0 - VEIL_REACH, 1.0, shell.reach);
    let wobble_point = world_position * 0.03;
    let wobble = vec2<f32>(
        value_noise_3d(vec3<f32>(wobble_point, globals.time * 0.3)),
        value_noise_3d(vec3<f32>(wobble_point + 7.0, globals.time * 0.3)),
    ) - 0.5;
    let lens = (-shell.direction * LENS * pow(1.0 - shell.height, 2.0) + wobble * 2.0 * WOBBLE * band) * smoothstep(0.0, 4.0, surface.boundary.edge);
    let offset = (surface.shift + lens) * field.progress;
    return vec4<f32>(world_offset_to_uv(offset), surface.burn * field.progress, 0.0);
}

fn overlay(in: VertexOutput, field: Instance) -> vec4<f32> {
    let world_position = in.world_position.xy;
    let time = globals.time;
    let shape = field_shapes[field.slot];
    if shape.current_radius <= 0.0 {
        return vec4<f32>(0.0);
    }
    let owner = field_owners_at(world_position).first.slot;

    // Let the outer strands extend onto nearby unclaimed ground.
    if owner == NO_FIELD {
        let shell = shell_at(world_position, shape.center, shape.current_radius);
        let edge = shape.current_radius - length(shell.offset);
        if -edge > OUTER_REACH { return vec4<f32>(0.0); }
        let light = shimmering_strands(shell, shell.around, edge, time, field.seed) * smoothstep(OUTER_REACH, OUTER_REACH * 0.6, -edge);
        return vec4<f32>(light * field.progress, 0.0);
    }
    if owner != field.slot {
        return vec4<f32>(0.0);
    }

    let surface = surface_at(world_position, field, shape, time, true);
    let shell = surface.shell;
    let seam = surface.boundary.first.distance;
    let inside = smoothstep(0.0, 2.0, surface.boundary.edge);
    // A seam is a standing wall between two domes, as tall as the dome above it.
    let wall_height = pow(shell.height, WALL_HEIGHT_POWER);
    let strand_weight = select(1.0, wall_height * STRAND_SEAM, seam < surface.boundary.rim);

    // The strands and veils lie behind the cuts and shift with them.
    let seen_shift = surface.shift * WEB_KINK;
    let seen_edge = surface.boundary.edge - dot(seen_shift, shell.direction);
    let seen_around = shell.around + dot(seen_shift, vec2<f32>(-shell.direction.y, shell.direction.x)) / max(length(shell.offset), 1.0);
    var light = shimmering_strands(shell, seen_around, seen_edge, time, field.seed) * strand_weight
              + veils(shell, seen_around, time, field.seed) * inside;
    light *= max(1.0 + surface.burn, 0.0);
    light += RIPPLE_COLOR * surface.ripple_glow * inside;

    let film = exp(-(seam * seam) / (FILM_WIDTH * FILM_WIDTH)) * FILM_OPACITY * 0.5;
    light += mix(NEUTRAL, display_to_linear(ICE_BLUE), FILM_TINT) * film * wall_height;
    return vec4<f32>(light * field.progress, 0.0);
}
