import package::shaders::common::clock::{CLOCK_WRAP_PERIOD, wrap_count, wrap_index};
import package::shaders::common::core::{CELL_SIZE, TAU};
import package::shaders::common::hash::{HASH_GOLDEN, hash_coords, hash_mix, hash_unit};
import package::shaders::common::map_info::grid_bounds;
import package::shaders::common::noise::{noise_2d, noise_2d_drift};

// Wave kinematics and glass-perimeter coordinates for the void sea.
// The glass perimeter is partitioned into fixed stretches (`HIT_SPACING`), each running an
// independent periodic wave schedule. A low-frequency swell field (`SET_SCALE`) modulates whether a
// spawned wave is a small lap or a large breaker.
//
// Driven by `globals.time`. Cyclic periods are quantized to `CLOCK_WRAP_PERIOD` (`common/clock.wesl`)
// and noise is advected via `common/noise.wesl` drift functions, keeping the wrap boundary seamless.

// Calm height of the sea's surface above the map, in world units.
const WATER_HEIGHT: f32 = 60.0;
// Peak wave surge height at the glass as a fraction of `WATER_HEIGHT`.
const WATER_SURGE: f32 = 0.6;

// Wave schedule (seconds and world units).
const IMPACT_PERIOD: f32 = 2.2;
// Target stretch length; quantized so an integer number of stretches fits the perimeter.
const HIT_SPACING: f32 = 90.0;
const HIT_WIDTH: f32 = 140.0;
const HIT_CHANCE: f32 = 0.1;
// Cycle fraction spent approaching the glass; the remainder (`1 - ARRIVE`) is the crash phase.
const ARRIVE: f32 = 0.5;
// Crash-phase fraction at which water surge peaks on the glass.
const HIT_PEAK: f32 = 0.2;
// Swell modulation wavelength along the glass and drift speed.
const SET_SCALE: f32 = 500.0;
const SET_SPEED: f32 = 0.06;
// Neighboring stretch radius checked per query (`ceil(0.9 * HIT_WIDTH / HIT_SPACING + 0.3)`).
const HIT_REACH: i32 = 2;
const HIT_SALT: u32 = 0x1f83d9abu;

fn floored_mod(x: f32, y: f32) -> f32 {
    return x - y * floor(x / y);
}

// The map's size in world units. The glass stands on its edge.
fn map_size() -> vec2<f32> {
    return vec2<f32>(grid_bounds()) * CELL_SIZE;
}

// Distance outside the glass, negative inside it.
fn glass_distance(p: vec2<f32>) -> f32 {
    let half_size = 0.5 * map_size();
    let e = abs(p - half_size) - half_size;
    return length(max(e, vec2<f32>(0.0))) + min(max(e.x, e.y), 0.0);
}

// Position along the glass, counter-clockwise from the map's bottom-left corner. Points off a corner
// take the corner's coordinate; points inside the map take the nearest side's.
// x = the coordinate, y = the glass's full length.
fn wall_coordinate(p: vec2<f32>) -> vec2<f32> {
    let size = map_size();
    let c = clamp(p, vec2<f32>(0.0), size);
    // Distances to the bottom, right, top and left sides.
    let to_side = vec4<f32>(c.y, size.x - c.x, size.y - c.y, c.x);
    let nearest = min(min(to_side.x, to_side.y), min(to_side.z, to_side.w));
    var s: f32;
    if to_side.x == nearest {
        s = c.x;
    } else if to_side.y == nearest {
        s = size.x + c.y;
    } else if to_side.z == nearest {
        s = size.x + size.y + (size.x - c.x);
    } else {
        s = 2.0 * size.x + size.y + (size.y - c.y);
    }
    return vec2<f32>(s, 2.0 * (size.x + size.y));
}

struct WallPoint {
    position: vec2<f32>,
    // Unit direction away from the map.
    outward: vec2<f32>,
};

// The point on the glass at coordinate `s`.
fn wall_point(s: f32) -> WallPoint {
    let size = map_size();
    var along = floored_mod(s, 2.0 * (size.x + size.y));
    if along < size.x {
        return WallPoint(vec2<f32>(along, 0.0), vec2<f32>(0.0, -1.0));
    }
    along -= size.x;
    if along < size.y {
        return WallPoint(vec2<f32>(size.x, along), vec2<f32>(1.0, 0.0));
    }
    along -= size.y;
    if along < size.x {
        return WallPoint(vec2<f32>(size.x - along, size.y), vec2<f32>(0.0, 1.0));
    }
    along -= size.x;
    return WallPoint(vec2<f32>(0.0, size.y - along), vec2<f32>(-1.0, 0.0));
}

// Value noise sampled around a circle so it is seamless along the closed perimeter. `scale` is the
// feature size in world units; `z` offsets the sample slice.
fn wall_noise(wall: vec2<f32>, scale: f32, z: f32) -> f32 {
    let angle = TAU * wall.x / wall.y;
    let ring = wall.y / (TAU * scale);
    return noise_2d(vec2<f32>(cos(angle), sin(angle)) * ring + vec2<f32>(z, 0.0));
}

// `wall_noise` drifting at `rate` lattice cells/s.
fn wall_noise_drift(wall: vec2<f32>, scale: f32, time: f32, rate: f32) -> f32 {
    let angle = TAU * wall.x / wall.y;
    let ring = wall.y / (TAU * scale);
    return noise_2d_drift(vec2<f32>(cos(angle), sin(angle)) * ring, time, vec2<f32>(rate, 0.0));
}

// State of a single perimeter stretch's current wave evaluated at a query point.
struct Hit {
    // Lateral falloff along the perimeter (`1` at `centre`, `0` beyond `half_width`).
    envelope: f32,
    // Wave intensity (`0..1`).
    strength: f32,
    // `envelope * strength` (`0` when inactive).
    weight: f32,
    // Approach progress (`0..1`) before impact, or `-1` after impact.
    incoming: f32,
    // Crash progress (`0..1`) after impact, or `-1` while approaching.
    crashed: f32,
    // Normalized vertical surge on the glass (`0..1`).
    rise: f32,
    // Perimeter coordinate of the wave's center and half-span.
    centre: f32,
    half_width: f32,
    seed: f32,
};

// Evaluates the active wave for the stretch at offset `k` from `wall.x`'s home stretch.
fn stretch_hit(wall: vec2<f32>, t: f32, k: i32) -> Hit {
    let none = Hit(0.0, 0.0, 0.0, -1.0, -1.0, 0.0, 0.0, 1.0, 0.0);
    let stretches = max(floor(wall.y / HIT_SPACING), 1.0);
    let spacing = wall.y / stretches;
    let base = floor(wall.x / spacing);
    let stretch = floored_mod(base + f32(k), stretches);
    let h0 = hash_coords(vec2<f32>(stretch, 0.0), HIT_SALT);
    let cycles = wrap_count(IMPACT_PERIOD * (0.6 + 0.8 * hash_unit(h0)));
    let period = CLOCK_WRAP_PERIOD / cycles;
    let offset = hash_unit(hash_mix(h0 ^ HASH_GOLDEN));
    let cycle = t / period + offset;
    // Wrap the generation index so an in-flight wave's hash does not jump when `t` wraps.
    let generation = wrap_index(floor(cycle), cycles);
    let age = fract(cycle);
    let h1 = hash_mix(h0 ^ (u32(generation) * HASH_GOLDEN));
    if hash_unit(h1) > HIT_CHANCE {
        return none;
    }
    let h2 = hash_mix(h1 + 1u);
    let h3 = hash_mix(h1 + 2u);
    let h4 = hash_mix(h1 + 3u);
    let h5 = hash_mix(h1 + 5u);
    let centre = (base + f32(k) + 0.5 + 0.6 * (hash_unit(h2) - 0.5)) * spacing;
    // big: 0 for a small lap, 1 for a big wave, from the swell energy where and when the wave is born.
    let born_at = (generation - offset) * period;
    let big = smoothstep(0.5, 0.82, wall_noise(vec2<f32>(centre, wall.y), SET_SCALE, born_at * SET_SPEED)) * (0.7 + 0.3 * hash_unit(h5));
    let strength = mix(0.12 + 0.3 * hash_unit(h4), 0.75 + 0.25 * hash_unit(h4), big);
    let half_width = 0.5 * HIT_WIDTH * mix(0.4 + 0.5 * hash_unit(h3), 1.2 + 0.6 * hash_unit(h3), big);
    let envelope = 1.0 - smoothstep(0.4, 1.0, abs(wall.x - centre) / half_width);
    let weight = envelope * strength;
    let incoming = select(-1.0, age / ARRIVE, age < ARRIVE);
    let crashed = select(-1.0, (age - ARRIVE) / (1.0 - ARRIVE), age >= ARRIVE);
    var rise = 0.0;
    if crashed >= 0.0 {
        if crashed < HIT_PEAK {
            rise = weight * (1.0 - pow(1.0 - crashed / HIT_PEAK, 2.0));
        } else {
            rise = weight * (1.0 - pow((crashed - HIT_PEAK) / (1.0 - HIT_PEAK), 2.0));
        }
    }
    return Hit(envelope, strength, weight, incoming, crashed, rise, centre, half_width, hash_unit(hash_mix(h1 + 4u)) * 97.0);
}

// How far the water stands above its calm level at the glass, in world units, from waves climbing it.
fn wall_lift(wall: vec2<f32>, t: f32) -> f32 {
    var rise = 0.0;
    for (var k = -HIT_REACH; k <= HIT_REACH; k++) {
        rise = max(rise, stretch_hit(wall, t, k).rise);
    }
    return WATER_HEIGHT * WATER_SURGE * rise;
}
