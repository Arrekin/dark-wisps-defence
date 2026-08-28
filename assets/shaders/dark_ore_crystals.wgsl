#define_import_path dwd::dark_ore_crystals
#import dwd::core::TAU
#import dwd::gradient_noise::dwd_gradient_fbm_2d
#import dwd::hash::{DWD_HASH_GOLDEN, dwd_hash_coords, dwd_hash_mix, dwd_hash_unit}
#import dwd::map_light::{MAP_SUN_DIRECTION, MAP_SUN_GROUND_DIRECTION, MAP_SUN_HALF_VECTOR}

// Binding-free procedural dark-ore shading. Callers provide distance, fill, screen scale and world
// position; nothing here declares a binding of its own.
//
// Each crystal is a deterministic leaning convex prism. A vertical orthographic view ray is clipped
// against its face half-spaces and the ground plane to find the visible surface. Nearby prisms are
// gathered from a world-space lattice and composited by surface height.
//
// Lighting is intentionally stylized: fixed material tones carry most of the form, while the shared
// map sun adds restrained diffuse, rim and specular terms. Shadows are contact-only.
//
// Constants follow one rule: shading response is named, shape distribution stays inline. A face
// colour, edge width or light coefficient gets a constant; the ranges a crystal's proportions are
// drawn from stay as literal `mix(low, high, roll)` so the shape reads where it is built.

const STAIN_LOW: vec3<f32> = vec3<f32>(0.0014, 0.0044, 0.0062);
const STAIN_HIGH: vec3<f32> = vec3<f32>(0.0038, 0.0124, 0.0174);

// Two-tone base material; `BODY_LIGHT` controls diffuse modulation by the map sun.
const BODY_LOW: vec3<f32> = vec3<f32>(0.0090, 0.0210, 0.0290);
const BODY_HIGH: vec3<f32> = vec3<f32>(0.0250, 0.0520, 0.0680);
const BODY_LIGHT: f32 = 0.55;

// Sky contribution to restrained face variation.
const SKY_LIGHT: f32 = 0.10;

// Root darkening: minimum brightness and smoothstep range in normalized rise.
const ROOT_DARKEN_FLOOR: f32 = 0.55;
const ROOT_DARKEN_RISE: f32 = 0.55;

const SHINE: vec3<f32> = vec3<f32>(0.4000, 0.8400, 0.9500);
const GLOSS: f32 = 44.0;

// Additive transmission colour and strength for thin crystal sections. `THROUGH_SPAN_OF_GIRTH` is
// the optical thickness, measured in crystal girths, at which transmission falls to zero.
const THROUGH: vec3<f32> = vec3<f32>(0.0180, 0.0620, 0.0810);
const THROUGH_STRENGTH: f32 = 1.75;
const THROUGH_SPAN_OF_GIRTH: f32 = 3.525;

// Brightness floor under full absorption, and transmission tint floor on faces away from the sun.
const ABSORPTION_FLOOR: f32 = 0.80;
const TRANSMISSION_FLOOR: f32 = 0.35;

// Growth-band contrast, world-height frequency and per-crystal phase separation.
const GRAIN_DEPTH: f32 = 0.30;
const GRAIN_SPACING: f32 = 0.26;
const GRAIN_OFFSET: f32 = 31.0;
const GRAIN_GAIN: f32 = 3.0;

// Crest accent, outer contour and inner depth-band colours.
const HAIRLINE: vec3<f32> = vec3<f32>(0.0400, 0.2600, 0.3600);
const CONTOUR: vec3<f32> = vec3<f32>(0.0020, 0.0034, 0.0048);
const INNER: vec3<f32> = vec3<f32>(0.0030, 0.0350, 0.0450);

// Ridge distance softening between the two nearest face ceilings.
const RIDGE_SOFTEN: f32 = 0.6;

// Crest accent multipliers: ambient, sun-keyed and shine contribution.
const CREST_AMBIENT: f32 = 0.30;
const CREST_KEY: f32 = 1.30;
const CREST_SHINE: f32 = 0.7;

// Outline opacity at the projected silhouette.
const OUTLINE_OPACITY: f32 = 0.85;

// Stylized silhouette rim opposite the sun. Band width is at least `RIM_TEXELS` screen pixels and
// at least `RIM_OF_GIRTH` of the crystal, capped at `RIM_CAP_OF_GIRTH` so the screen-pixel floor
// cannot cover a distant crystal outright.
const RIM: vec3<f32> = vec3<f32>(0.0420, 0.1450, 0.1950);
const RIM_TEXELS: f32 = 3.0;
const RIM_OF_GIRTH: f32 = 0.30;
const RIM_CAP_OF_GIRTH: f32 = 0.45;

// Nominal crest and outline width in screen pixels. Per-crystal girth caps prevent small crystals
// from becoming entirely edge.
const EDGE_TEXELS: f32 = 1.3;

// Projected crystal width in screen pixels over which crest, outline, rim, lustre, growth bands and
// the inner depth band fade in. Narrower than `DETAIL_HIGH_TEXELS` a crystal cannot carry an
// `EDGE_TEXELS` line and a rim at once, and sampling them yields speckle rather than detail. Body
// tone, plate noise, root darkening and transmission are exempt: they hold at any zoom, so a
// distant crystal keeps its coverage as a smooth ore-toned dot. A typical crystal is wider than
// `DETAIL_HIGH_TEXELS` at full zoom-in.
const DETAIL_LOW_TEXELS: f32 = 1.0;
const DETAIL_HIGH_TEXELS: f32 = 3.0;

// How much of a crystal's own width an edge may take, for the crease and for the outline.
const CREST_OF_GIRTH: f32 = 0.26;
const OUTLINE_OF_GIRTH: f32 = 0.30;

// Crystal-surface noise scale in world pixels and remapping gain.
const PLATE_SCALE: f32 = 21.0;
const PLATE_GAIN: f32 = 2.2;

// World-length range over which specular lustre reaches full strength. Length is used instead of
// width so long, narrow crystals retain highlights.
const LUSTRE_GATE_LOW: f32 = 7.0;
const LUSTRE_GATE_HIGH: f32 = 30.0;

// Minimum lustre carry on short crystals.
const LUSTRE_FLOOR: f32 = 0.30;

// Inner depth-band width and the girth range that suppresses it on very small crystals.
const DEPTH_BAND: f32 = 3.0;
const DEPTH_OF_GIRTH: f32 = 0.42;
const DEPTH_GATE_LOW: f32 = 3.0;
const DEPTH_GATE_HIGH: f32 = 10.0;

// How quickly the stain reaches full strength inside the deposit's boundary.
const STAIN_FADE: f32 = 4.0;

// Minimum stain colour modulation at zero fill.
const STAIN_FLOOR: f32 = 0.55;

// Base stain-mottle lattice size in world pixels; intentionally independent of the map-cell grid.
const MOTTLE_SCALE: f32 = 26.0;

// The boundary is inset at two scales, always inward. Scales and amounts are world pixels; the
// narrow range preserves the interior of one-cell-thick shapes.
const INSET_BASE: f32 = 0.5;
const INSET_SCALE: f32 = 44.0;
const INSET_AMOUNT: f32 = 2.0;
const INSET_FINE_SCALE: f32 = 13.0;
const INSET_FINE_AMOUNT: f32 = 0.75;
const INSET_GAIN: f32 = 2.2;

// Crystal start inset and full-size ramp width, in world pixels.
const CRYSTAL_INSET: f32 = 2.0;
const CRYSTAL_REACH: f32 = 10.0;

// Minimum crystal scale at the deposit boundary.
const SCALE_FLOOR: f32 = 0.6;

// Zero-fill endpoint of the abundance ramp; the effective minimum also includes
// `MINIMUM_VISIBLE_FILL`. Mining removes crystals deterministically instead of dimming them.
const ABUNDANCE_SPENT: f32 = 0.12;

// Contact radius per world-height, probe offset in world pixels, and root-relative height cutoff.
const CONTACT_PER_HEIGHT: f32 = 0.55;
const CONTACT_OFFSET: f32 = 1.6;
const ROOT_SPAN: f32 = 0.34;

// Contact reach clamp bounds in world pixels.
const CONTACT_REACH_MIN: f32 = 1.5;
const CONTACT_REACH_MAX: f32 = 8.0;

// Narrowing of the offset probe's effective range relative to the local probe.
const OFFSET_REACH_FACTOR: f32 = 0.7;

// Blend weights for local and offset contact probes.
const HUG_WEIGHT: f32 = 0.52;
const OFFSET_WEIGHT: f32 = 0.50;

// Contact alpha floor and covered-surface darkening from the offset probe.
const CONTACT_ALPHA: f32 = 0.8;
const CONTACT_SHADOW: f32 = 0.30;

// Fraction of total crystal length below ground. A uniform fraction keeps neighboring crystals
// anchored to a consistent ground plane.
const BURIAL: f32 = 0.26;

// Crystal-seat lattice spacing and large-scale crowding variation, in world pixels.
const PRISM_SCALE: f32 = 6.5;
const CROWDING_SCALE: f32 = 70.0;

// Seat occupancy ramp endpoints; see `ore_crowding` for why the top is not reachable.
const CROWDING_BASE: f32 = 0.80;
const CROWDING_TOP: f32 = 1.38;

// Root radius as a fraction of half the nearest-seat distance. Values above one allow limited
// overlap to close visible gaps.
const ROOM_MIN: f32 = 0.95;
const ROOM_MAX: f32 = 1.15;

// Lean angle range in radians. The power distribution favors upright crystals so strongly leaning
// shafts do not dominate the top-down silhouette.
const LEAN_MIN: f32 = 0.10;
const LEAN_MAX: f32 = 0.82;
const LEAN_FALLOFF: f32 = 1.9;

// Bearing strata per square lattice block, and block width in lattice cells. `SECTORS` has to stay
// `BLOCK * BLOCK` for the stride permutation in `bearing_turn` to visit every sector exactly once.
const SECTORS: i32 = 16;
const BLOCK: f32 = 4.0;

// Neighbourhood radius in lattice cells. Above-ground reach from a seat is
// `(shaft + cap) * (sin(lean) - BURIAL * tan(lean))`, peaking near 19 world pixels — about 2.9
// cells — at lean 0.68, so two clips the tips of the longest strongly leaning crystals; the bed is
// dense enough to cover it. Re-derive from the radius, shaft and lean ranges when those change.
const GATHER_RING: i32 = 2;

// Maximum generated prism side count; also bounds the face-clipping loop.
const MAX_SIDES: i32 = 6;

// Procedural termination variants.
const TOP_POINT: i32 = 0;
const TOP_BROKEN: i32 = 1;
const TOP_ALTERNATING: i32 = 2;
const TOP_CHISEL: i32 = 3;

// Minimum normalized fill used for shading while an ore entity still exists.
const MINIMUM_VISIBLE_FILL: f32 = 0.05;

// Finite sentinel for world-space distance and height extrema.
const FAR: f32 = 1.0e30;

// --------------------------------------------------------------------------------------------
// Noise
// --------------------------------------------------------------------------------------------

// Mean of `dwd_gradient_fbm_2d` for `octaves` amplitudes starting at 0.5 and halving each octave.
fn fbm_mean(octaves: i32) -> f32 {
    return 0.5 * (1.0 - pow(0.5, f32(octaves)));
}

// Recenters FBM around its octave-dependent mean and expands its narrow distribution into [0, 1].
fn noise_blend(value: f32, octaves: i32, gain: f32) -> f32 {
    return saturate((value - fbm_mean(octaves)) * gain + 0.5);
}

// Random values are drawn from a lattice cell and a salt. Two draws that read the same cell come
// out independent only if their salts differ, so every salt is used exactly once:
//
//   50-51  seat position and seat occupancy
//   60-65  per-face reach
//   70-91  root and prism shape, and the gather rank
//
// A new draw therefore needs a salt that does not already appear in this file. Salts 88 and 89 in
// `bearing_turn` are the ones to watch: they read a block coordinate, not a cell, and a block
// coordinate can land on some other cell's coordinate. Their salt being unused elsewhere is the
// only thing keeping those two draws apart.

// Two deterministic values in [0, 1).
fn ore_hash(cell: vec2<f32>, salt: u32) -> vec2<f32> {
    let base = dwd_hash_coords(cell, DWD_HASH_GOLDEN ^ (salt * 0x85ebca6bu));
    return vec2<f32>(dwd_hash_unit(base), dwd_hash_unit(dwd_hash_mix(base ^ 0x68bc21ebu)));
}

// One deterministic value in [0, 1); the `x` of `ore_hash` under the same salt.
fn ore_roll(cell: vec2<f32>, salt: u32) -> f32 {
    return dwd_hash_unit(dwd_hash_coords(cell, DWD_HASH_GOLDEN ^ (salt * 0x85ebca6bu)));
}

// How far the visible deposit is carved inward from the logical union of ore cells. Both terms are
// stretched across the noise's useful range and remain non-negative, so the outline can wander
// inside an occupied cell but can never invade an empty one.
fn ore_inset(world: vec2<f32>) -> f32 {
    let coarse = noise_blend(dwd_gradient_fbm_2d(world / INSET_SCALE, 3), 3, INSET_GAIN);
    let fine = noise_blend(dwd_gradient_fbm_2d(world / INSET_FINE_SCALE + vec2<f32>(11.3, 57.8), 2), 2, INSET_GAIN);
    return INSET_BASE + coarse * INSET_AMOUNT + fine * INSET_FINE_AMOUNT;
}

// --------------------------------------------------------------------------------------------
// Crystal placement and root footprint
// --------------------------------------------------------------------------------------------

fn seat_of(cell: vec2<f32>) -> vec2<f32> {
    return (cell + ore_hash(cell, 50u)) * PRISM_SCALE;
}

// Returns half the nearest-seat distance, capped at half a lattice cell. Using this as the root
// radius bounds neighboring footprints without rejecting crowded seats.
fn room_at(cell: vec2<f32>, seat: vec2<f32>) -> f32 {
    // Seeding the search with the cap makes seats outside the scanned 3x3 neighborhood irrelevant
    // to footprint overlap. Ranking by squared distance defers the one square root to the result.
    var nearest_squared = PRISM_SCALE * PRISM_SCALE;

    for (var y = -1; y <= 1; y++) {
        for (var x = -1; x <= 1; x++) {
            if x == 0 && y == 0 {
                continue;
            }
            let delta = seat_of(cell + vec2<f32>(f32(x), f32(y))) - seat;
            nearest_squared = min(nearest_squared, dot(delta, delta));
        }
    }

    return sqrt(nearest_squared) * 0.5;
}

// Probability that a seat holds a crystal. Varies well above a cell so the crowding is a property
// of the deposit rather than of the lattice the crystals are seated on. `CROWDING_TOP` sets the
// ramp's slope, not a reachable value: the ramp hits 1.0 — every seat filled — at 0.345 of the
// noise range and is clamped flat above that.
fn ore_crowding(world: vec2<f32>) -> f32 {
    return min(mix(CROWDING_BASE, CROWDING_TOP, saturate(dwd_gradient_fbm_2d(world / CROWDING_SCALE, 3))), 1.0);
}

// Whether a cell holds a crystal at all. Kept clear of the spacing above, which is settled for
// every seat whether or not it is occupied, so that thinning the bed never resizes what is left.
fn ore_present(cell: vec2<f32>) -> bool {
    let centre = (cell + vec2<f32>(0.5)) * PRISM_SCALE;
    return ore_roll(cell, 51u) <= ore_crowding(centre);
}

// Stratifies lean bearings across each 4x4 lattice block. A per-block odd stride permutes all 16
// sectors exactly once; a random turn and per-crystal jitter hide the block pattern.
fn bearing_turn(cell: vec2<f32>) -> f32 {
    let block = floor(cell / BLOCK);
    let local = cell - block * BLOCK;
    let index = i32(local.y) * i32(BLOCK) + i32(local.x);

    let stride = 1 + 2 * i32(ore_roll(block, 88u) * (f32(SECTORS) * 0.5));
    let turn = i32(ore_roll(block, 89u) * f32(SECTORS));
    // Every term is non-negative by construction, so a plain remainder is a modulo here.
    let sector = (index * stride + turn) % SECTORS;

    return (f32(sector) + ore_roll(cell, 75u)) / f32(SECTORS);
}

// Where a crystal meets the ground, and how much of the ground it takes.
struct Root {
    cell: vec2<f32>,
    seat: vec2<f32>,
    radius: f32,
    lean: f32,
}

fn root_of(cell: vec2<f32>) -> Root {
    let lean = mix(LEAN_MIN, LEAN_MAX, pow(ore_roll(cell, 74u), LEAN_FALLOFF));

    // Limit the preferred radius so the ground-plane footprint remains within the available room.
    // The `cos(lean)` factor accounts for the footprint stretching along the lean direction.
    let seat = seat_of(cell);
    let want = mix(1.5, 4.2, pow(ore_roll(cell, 70u), 1.35));
    let allowed = room_at(cell, seat) * mix(ROOM_MIN, ROOM_MAX, ore_roll(cell, 71u)) * cos(lean);

    var root: Root;
    root.cell = cell;
    root.seat = seat;
    root.radius = min(want, allowed);
    root.lean = lean;
    return root;
}

// --------------------------------------------------------------------------------------------
// Prism geometry
// --------------------------------------------------------------------------------------------

// Convex prism geometry in world-pixel units, extruded along a unit axis and closed by a termination.
struct Prism {
    cell: vec2<f32>, // Lattice cell the crystal is seated in; the source of every roll below.
    base: vec3<f32>, // Axis origin; z is below the ground plane.
    axis: vec3<f32>,
    spin: f32,
    radius: f32,
    shaft: f32,
    cap: f32,
    skew: vec2<f32>, // Termination point offset in the prism's cross-section frame.
    top: i32,
    taper: f32, // Fraction of root width lost by the termination.
    cut: f32, // Fractional cut position for broken and chisel terminations.
    sides: i32,
    height: f32, // Length above the ground plane.
    tone: f32,
}

// Builds the spun orthonormal frame perpendicular to the prism axis.
fn prism_frame(prism: Prism) -> mat2x3<f32> {
    var aside = vec3<f32>(1.0, 0.0, 0.0);
    if abs(prism.axis.z) < 0.9 {
        aside = vec3<f32>(0.0, 0.0, 1.0);
    }
    let first = normalize(cross(prism.axis, aside));
    let second = cross(prism.axis, first);
    let turn = sin(prism.spin);
    let hold = cos(prism.spin);
    return mat2x3<f32>(first * hold + second * turn, second * hold - first * turn);
}

fn prism_face_reach(cell: vec2<f32>, radius: f32, index: i32) -> f32 {
    return radius * mix(0.74, 1.26, ore_roll(cell, 60u + u32(index)));
}

// Constructs deterministic prism dimensions, orientation and termination from a root.
fn prism_of(root: Root) -> Prism {
    let cell = root.cell;
    let radius = root.radius;

    // Long shafts remain visibly elongated after top-down projection shortens them by the lean angle.
    let shaft = radius * mix(7.0, 13.0, ore_roll(cell, 72u));
    let cap = radius * mix(1.2, 2.6, ore_roll(cell, 73u));

    let lean = root.lean;
    let bearing = bearing_turn(root.cell) * TAU;
    let reach = tan(lean);
    let axis = normalize(vec3<f32>(cos(bearing) * reach, sin(bearing) * reach, 1.0));

    // Reduce chisel-top probability on upright crystals, where the cut faces the camera and obscures
    // the shaft.
    let square_on = 1.0 - smoothstep(0.30, 0.62, lean);
    let draw = ore_roll(cell, 76u) * mix(1.0, 0.84, square_on);
    var top = TOP_CHISEL;
    if draw < 0.42 {
        top = TOP_POINT;
    } else if draw < 0.64 {
        top = TOP_ALTERNATING;
    } else if draw < 0.84 {
        top = TOP_BROKEN;
    }

    let skew_bearing = ore_roll(cell, 77u) * TAU;
    let skew = vec2<f32>(cos(skew_bearing), sin(skew_bearing)) * radius * mix(0.0, 0.30, ore_roll(cell, 78u));

    let buried = (shaft + cap) * BURIAL;

    // Back-project from the ground seat so burial does not move the emergence point of leaning prisms.
    let below = axis.xy * (buried / axis.z);

    var prism: Prism;
    prism.cell = cell;
    prism.base = vec3<f32>(root.seat - below, -buried);
    prism.axis = axis;
    prism.spin = ore_roll(cell, 80u) * TAU;
    prism.radius = radius;
    prism.shaft = shaft;
    prism.cap = cap;
    prism.skew = skew;
    prism.top = top;
    prism.taper = mix(0.03, 0.46, pow(ore_roll(cell, 91u), 1.5));
    prism.cut = mix(0.15, 0.60, ore_roll(cell, 81u));
    prism.sides = 4 + i32(ore_roll(cell, 82u) * 3.0);
    prism.height = shaft + cap - buried;
    prism.tone = mix(0.82, 1.18, ore_roll(cell, 84u));
    return prism;
}

// A prism placed and scaled for one gather; every length is already multiplied by `scale`. Shared
// by the view ray and the contact probe, which need the same frame and apex.
struct Pose {
    across: vec3<f32>,
    along: vec3<f32>,
    base: vec3<f32>,
    apex: vec3<f32>,
    shaft: f32,
    cap: f32,
    radius: f32,
    height: f32, // Length above the ground plane.
}

// The crystal shrinks about its own seat rather than moving, so a deposit thinning toward its
// boundary keeps its crystals where they were.
fn prism_pose(prism: Prism, scale: f32) -> Pose {
    let frame = prism_frame(prism);
    let across = frame[0];
    let along = frame[1];

    let base = vec3<f32>(prism.base.xy, prism.base.z * scale);
    let shaft = prism.shaft * scale;
    let cap = prism.cap * scale;
    let apex = base + prism.axis * (shaft + cap)
        + across * (prism.skew.x * scale) + along * (prism.skew.y * scale);

    return Pose(across, along, base, apex, shaft, cap, prism.radius * scale, prism.height * scale);
}

// Bounds of a vertical view ray clipped against the prism half-spaces. `ceiling` is the visible
// surface; `second` supplies ridge distance and `floor` supplies optical thickness.
struct Clip {
    ceiling: f32,
    second: f32,
    floor: f32,
    face: vec3<f32>,
    outside: bool,
}

// Intersects the vertical ray through `point` with `dot(normal, position) <= offset`.
fn clip_plane(clip: Clip, normal: vec3<f32>, offset: f32, point: vec2<f32>) -> Clip {
    var out = clip;
    if out.outside {
        return out;
    }
    let rest = offset - normal.x * point.x - normal.y * point.y;
    if normal.z > 1.0e-4 {
        let height = rest / normal.z;
        if height < out.ceiling {
            out.second = out.ceiling;
            out.ceiling = height;
            out.face = normal;
        } else if height < out.second {
            out.second = height;
        }
    } else if normal.z < -1.0e-4 {
        out.floor = max(out.floor, rest / normal.z);
    } else if rest < 0.0 {
        out.outside = true;
    }
    return out;
}

struct Sample {
    hit: bool,
    distance: f32, // Approximate signed distance to the projected silhouette.
    normal: vec3<f32>, // Visible face normal.
    ridge: f32, // Separation between the nearest two ray ceilings.
    rise: f32, // Normalized visible height used for root-to-tip shading.
    surface: f32, // Visible world-space z; orders overlapping prisms.
    ground: f32, // Axis height at the nearest projected silhouette point.
    thickness: f32, // Ray entry-to-exit depth; equals `surface` while the ground is the only downward-facing plane.
    outward: vec2<f32>, // Planar direction from the projected axis to this point.
}

fn empty_sample() -> Sample {
    var sample: Sample;
    sample.hit = false;
    sample.distance = FAR;
    sample.normal = vec3<f32>(0.0, 0.0, 1.0);
    sample.ridge = FAR;
    sample.rise = 0.0;
    sample.surface = -FAR;
    sample.ground = FAR;
    sample.thickness = 0.0;
    sample.outward = vec2<f32>(0.0);
    return sample;
}

// Reduced projected-axis sample used by the contact probe without clipping prism faces.
struct Proximity {
    distance: f32,
    ground: f32,
}

// A capsule around the projected axis approximates silhouette distance for outlines and contact.
struct AxisProjection {
    along: f32, // Segment parameter, 0 at the base and 1 at the apex.
    offset: vec2<f32>, // Planar vector from the nearest axis point to the query point.
    reach: f32, // Length of `offset`.
}

fn axis_project(pose: Pose, point: vec2<f32>) -> AxisProjection {
    let span = pose.apex.xy - pose.base.xy;
    let span_squared = dot(span, span);
    var along = 0.0;
    if span_squared > 1.0e-6 {
        along = saturate(dot(point - pose.base.xy, span) / span_squared);
    }
    let offset = point - (pose.base.xy + span * along);
    return AxisProjection(along, offset, length(offset));
}

fn proximity_at(pose: Pose, point: vec2<f32>) -> Proximity {
    let axis = axis_project(pose, point);
    return Proximity(
        axis.reach - pose.radius,
        mix(pose.base.z, pose.apex.z, axis.along),
    );
}

// Clips the vertical view ray at `point` against a scaled prism and derives its shading sample.
fn prism_at(prism: Prism, pose: Pose, point: vec2<f32>) -> Sample {
    let across = pose.across;
    let along = pose.along;
    let base = pose.base;
    let apex = pose.apex;
    let shaft = pose.shaft;
    let cap = pose.cap;

    var clip: Clip;
    clip.ceiling = FAR;
    clip.second = FAR;
    clip.floor = -FAR;
    clip.face = vec3<f32>(0.0, 0.0, 1.0);
    clip.outside = false;

    // The ground plane closes the buried shaft, so no separate bottom cap is needed.
    clip = clip_plane(clip, vec3<f32>(0.0, 0.0, -1.0), 0.0, point);

    for (var index = 0; index < MAX_SIDES; index++) {
        if index >= prism.sides {
            break;
        }
        let angle = f32(index) * TAU / f32(prism.sides);
        let outward = across * cos(angle) + along * sin(angle);
        let reach = prism_face_reach(prism.cell, pose.radius, index);

        // Each tapered side plane passes through its root-width edge and tilts inward along the axis.
        // Keeping the root edge fixed preserves the footprint spacing constraint.
        let slope = prism.taper * reach / (shaft + cap);
        let side = normalize(outward + prism.axis * slope);
        clip = clip_plane(clip, side, dot(side, base + outward * reach), point);

        // Build each termination plane through its tapered shaft edge and the shared apex so the
        // cap meets the side without crossing it.
        var edge = shaft;
        if prism.top == TOP_ALTERNATING && index % 2 == 1 {
            edge = shaft + cap * 0.40;
        }
        let edge_reach = reach - slope * edge;
        let run = max(shaft + cap - edge, 0.05);
        let top_normal = normalize(outward * run + prism.axis * edge_reach);
        clip = clip_plane(clip, top_normal, dot(top_normal, apex), point);
    }

    // Broken and chisel variants truncate the termination with an additional clipping plane.
    if prism.top == TOP_BROKEN || prism.top == TOP_CHISEL {
        var tipped = prism.axis;
        if prism.top == TOP_BROKEN {
            tipped = normalize(prism.axis + across * 0.45 + along * 0.25);
        }
        let cut_at = base + prism.axis * (shaft + cap * prism.cut);
        clip = clip_plane(clip, tipped, dot(tipped, cut_at), point);
    }

    let missed = clip.outside || clip.ceiling < clip.floor || clip.ceiling >= FAR;

    let axis = axis_project(pose, point);

    var sample: Sample;
    sample.hit = !missed;
    sample.distance = axis.reach - pose.radius;
    sample.normal = clip.face;
    sample.ridge = max((clip.second - clip.ceiling) * RIDGE_SOFTEN, 0.0);
    sample.rise = saturate(clip.ceiling / max(shaft + cap, 0.01));
    sample.surface = clip.ceiling;
    // Orthographic projection preserves the axis parameter used to recover ground-relative height.
    sample.ground = mix(base.z, apex.z, axis.along);
    sample.thickness = max(clip.ceiling - clip.floor, 0.0);
    sample.outward = select(vec2<f32>(0.0), axis.offset / max(axis.reach, 1.0e-4), axis.reach > 1.0e-4);
    return sample;
}

// --------------------------------------------------------------------------------------------
// Gather and composition
// --------------------------------------------------------------------------------------------

// Nearest projected silhouette in one probe, with the dimensions of the prism that owns it. Both
// the primary point and the offset probe are shaded from this alone.
struct Contact {
    nearest: f32, // Nearest projected silhouette distance, hit or miss.
    nearest_height: f32,
    nearest_ground: f32,
}

struct Look {
    contact: Contact,
    sample: Sample, // `sample.hit` is false until a prism covers this point. `surface` is the highest visible z.
    tone: f32,
    girth: f32, // Visible crystal diameter; caps line widths.
    presence: f32, // Scaled axis length; gates specular strength.
}

// Primary visible sample plus the offset contact probe gathered from the same candidate prisms.
struct View {
    look: Look,
    contact: Contact,
}

fn empty_contact() -> Contact {
    var contact: Contact;
    contact.nearest = FAR;
    contact.nearest_height = 1.0;
    contact.nearest_ground = FAR;
    return contact;
}

fn empty_look() -> Look {
    var look: Look;
    look.contact = empty_contact();
    look.sample = empty_sample();
    look.tone = 1.0;
    look.girth = 1.0;
    look.presence = 1.0;
    return look;
}

fn absorb_contact(contact: Contact, proximity: Proximity, pose: Pose) -> Contact {
    var out = contact;
    if proximity.distance < out.nearest {
        out.nearest = proximity.distance;
        out.nearest_height = pose.height;
        out.nearest_ground = proximity.ground;
    }
    return out;
}

fn absorb(look: Look, sample: Sample, prism: Prism, pose: Pose) -> Look {
    var out = look;
    out.contact = absorb_contact(out.contact, Proximity(sample.distance, sample.ground), pose);
    // A later prism replaces an earlier one only where its visible surface is higher.
    if sample.hit && sample.surface > out.sample.surface {
        out.girth = pose.radius * 2.0;
        out.presence = pose.shaft + pose.cap;
        out.tone = prism.tone;
        out.sample = sample;
    }
    return out;
}

// Gathers the union of primary and contact neighborhoods so shared candidates are constructed once.
fn look_at(point: vec2<f32>, contact_point: vec2<f32>, abundance: f32, scale: f32) -> View {
    var view: View;
    view.look = empty_look();
    view.contact = empty_contact();

    let point_cell = vec2<i32>(floor(point / PRISM_SCALE));
    let contact_cell = vec2<i32>(floor(contact_point / PRISM_SCALE));
    let first = min(point_cell, contact_cell) - vec2<i32>(GATHER_RING);
    let last = max(point_cell, contact_cell) + vec2<i32>(GATHER_RING);

    for (var y = first.y; y <= last.y; y++) {
        for (var x = first.x; x <= last.x; x++) {
            let coords = vec2<i32>(x, y);
            let point_delta = abs(coords - point_cell);
            let contact_delta = abs(coords - contact_cell);
            let sees_point = point_delta.x <= GATHER_RING && point_delta.y <= GATHER_RING;
            let sees_contact = contact_delta.x <= GATHER_RING && contact_delta.y <= GATHER_RING;
            if !sees_point && !sees_contact {
                continue;
            }

            let here = vec2<f32>(coords);
            // Stable rank makes depletion deterministic. `>=` ensures rank zero is rejected when
            // boundary thinning reduces abundance to zero.
            let rank = ore_roll(here, 85u);
            if rank >= abundance || !ore_present(here) {
                continue;
            }
            let prism = prism_of(root_of(here));
            let pose = prism_pose(prism, scale);
            if sees_point {
                view.look = absorb(view.look, prism_at(prism, pose, point), prism, pose);
            }
            if sees_contact {
                view.contact = absorb_contact(view.contact, proximity_at(pose, contact_point), pose);
            }
        }
    }

    return view;
}

// Restricts contact darkening to silhouette sections near the ground plane.
fn bedded(ground: f32, height: f32) -> f32 {
    return smoothstep(height * ROOT_SPAN, 0.0, max(ground, 0.0));
}

// Silhouette contact strength, over a reach set by the height of the crystal that owns the nearest
// silhouette. `reach_factor` narrows that reach for probes taken away from the shaded point.
fn contact_strength(contact: Contact, reach_factor: f32) -> f32 {
    let reach = clamp(contact.nearest_height * CONTACT_PER_HEIGHT, CONTACT_REACH_MIN, CONTACT_REACH_MAX);
    return smoothstep(reach * reach_factor, 0.0, max(contact.nearest, 0.0))
        * bedded(contact.nearest_ground, contact.nearest_height);
}

// --------------------------------------------------------------------------------------------
// Shading
// --------------------------------------------------------------------------------------------

// Composites stain, contact, crystal material, transmission, bands, highlights and line work.
fn dark_ore_shading(d: f32, fill: f32, texel: f32, world: vec2<f32>) -> vec4<f32> {
    let visible_fill = max(saturate(fill), MINIMUM_VISIBLE_FILL);

    // Fill controls deterministic abundance; distance additionally thins and scales crystals near
    // the deposit boundary without moving their seats.
    let interior = smoothstep(0.0, CRYSTAL_REACH, d - CRYSTAL_INSET);
    let abundance = mix(ABUNDANCE_SPENT, 1.0, visible_fill) * interior;
    let scale = mix(SCALE_FLOOR, 1.0, interior);

    let view = look_at(world, world - MAP_SUN_GROUND_DIRECTION * CONTACT_OFFSET, abundance, scale);
    let here = view.look;

    // The short probe opposite the sun; read by both the ground and the crystal branch.
    let offset = contact_strength(view.contact, OFFSET_REACH_FACTOR);

    if !here.sample.hit {
        // Stain is only needed where no crystal covers the ground, and so is the local probe.
        let contact = saturate(contact_strength(here.contact, 1.0) * HUG_WEIGHT + offset * OFFSET_WEIGHT);
        // Read raw rather than through `noise_blend`: the stain keeps to the middle of its colour range.
        let mottle = saturate(dwd_gradient_fbm_2d(world / MOTTLE_SCALE, 4));
        let stain_colour = mix(STAIN_LOW, STAIN_HIGH, mottle * mix(STAIN_FLOOR, 1.0, visible_fill));
        let stain = smoothstep(0.0, STAIN_FADE, d);
        // Contact alpha remains visible where the underlying stain is faint.
        return vec4<f32>(stain_colour * (1.0 - contact), max(stain, contact * CONTACT_ALPHA));
    }

    let sample = here.sample;
    let key = saturate(dot(sample.normal, MAP_SUN_DIRECTION));
    let sky = saturate(0.5 + 0.5 * sample.normal.z);

    // Projected width of the visible crystal, in screen pixels, remapped to scale every layer finer
    // than the crystal itself.
    let detail = smoothstep(DETAIL_LOW_TEXELS, DETAIL_HIGH_TEXELS, here.girth / texel);

    // Material tones dominate; diffuse and sky terms provide restrained face variation.
    let plate = noise_blend(dwd_gradient_fbm_2d(world / PLATE_SCALE, 3), 3, PLATE_GAIN);
    var stone = mix(BODY_LOW, BODY_HIGH, plate) * (1.0 + BODY_LIGHT * key + SKY_LIGHT * sky) * here.tone;

    // Darken surfaces near the root to integrate the shaft with the bed.
    stone = stone * mix(ROOT_DARKEN_FLOOR, 1.0, smoothstep(0.0, ROOT_DARKEN_RISE, sample.rise));

    // Approximate transmission from vertical optical thickness normalized by crystal girth. Thin
    // silhouettes brighten; thick sections receive mild absorption.
    let through = 1.0 - smoothstep(0.0, here.girth * THROUGH_SPAN_OF_GIRTH, sample.thickness);
    stone = stone * mix(ABSORPTION_FLOOR, 1.0, through);
    stone = stone + THROUGH * (through * through * mix(TRANSMISSION_FLOOR, 1.0, key) * THROUGH_STRENGTH);

    // Growth bands use world height for size-independent spacing. Crystal tone provides a stable
    // second noise coordinate that decorrelates neighboring bands.
    let bands = noise_blend(dwd_gradient_fbm_2d(vec2<f32>(sample.surface * GRAIN_SPACING, here.tone * GRAIN_OFFSET), 3), 3, GRAIN_GAIN);
    stone = stone * mix(1.0 - GRAIN_DEPTH * detail, 1.0 + GRAIN_DEPTH * detail, bands);

    // The lustre, gated on length rather than width.
    let carries = mix(LUSTRE_FLOOR, 1.0, smoothstep(LUSTRE_GATE_LOW, LUSTRE_GATE_HIGH, here.presence));
    let shine = pow(saturate(dot(sample.normal, MAP_SUN_HALF_VECTOR)), GLOSS) * carries;
    stone = stone + SHINE * (shine * detail);

    let thin = 1.0 - smoothstep(0.0, min(here.girth * DEPTH_OF_GIRTH, DEPTH_BAND), abs(sample.distance));
    stone = stone + INNER * (thin * thin * smoothstep(DEPTH_GATE_LOW, DEPTH_GATE_HIGH, here.girth) * detail);

    // Use the projected silhouette direction for the rim; top-facing facet normals contain little
    // useful planar direction in an orthographic view.
    let away = saturate(-dot(sample.outward, MAP_SUN_GROUND_DIRECTION));
    let rim_reach = min(max(texel * RIM_TEXELS, here.girth * RIM_OF_GIRTH), here.girth * RIM_CAP_OF_GIRTH);
    let rim_band = 1.0 - smoothstep(0.0, rim_reach, abs(sample.distance));
    stone = stone + RIM * (away * away * rim_band * detail);

    // Derive line widths in screen space, then cap them by crystal girth.
    let crest_width = min(texel * EDGE_TEXELS, here.girth * CREST_OF_GIRTH);
    let rim_width = min(texel * EDGE_TEXELS, here.girth * OUTLINE_OF_GIRTH);

    // Crest accent follows the nearest competing face and brightens toward the sun.
    let crest = 1.0 - smoothstep(0.0, crest_width, sample.ridge);
    let lit_edge = HAIRLINE * (CREST_AMBIENT + CREST_KEY * key) + SHINE * (shine * CREST_SHINE);
    stone = mix(stone, lit_edge, crest * detail);

    let outline = 1.0 - smoothstep(0.0, rim_width, abs(sample.distance));
    stone = mix(stone, CONTOUR, outline * OUTLINE_OPACITY * detail);

    // Apply the offset contact term to overlapping crystals as well as the ground.
    return vec4<f32>(stone * (1.0 - offset * CONTACT_SHADOW), 1.0);
}
