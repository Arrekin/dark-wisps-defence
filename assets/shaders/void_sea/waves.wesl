import package::shaders::common::core::{CELL_SIZE, TAU};
import package::shaders::common::hash::{HASH_GOLDEN, hash_coords, hash_mix, hash_unit};
import package::shaders::common::map_info::grid_bounds;
import package::shaders::common::value_noise::value_noise_2d;

// Waves on the glass around the map. Each stretch of the glass gets waves on its own random schedule: a
// wave runs in from the sea, crashes into the glass and dies there. Most waves are small; now and then a
// big one comes. A slow swell energy drifts along the glass and decides where the big ones land.
//
// Time is Bevy's shader clock, which wraps to 0 every `TIME_WRAP` seconds. Every repeat fits a whole
// number of times into the wrap, and drifting noise crossfades into its own state at time 0 before it,
// so the wrap shows no seam.

// Mirrors Bevy's default `Time::wrap_period`, which wraps `globals.time`.
const TIME_WRAP: f32 = 3600.0;
// Seconds before the wrap over which drifting noise crossfades into its state at time 0.
const WRAP_FADE: f32 = 20.0;

// Calm height of the sea's surface above the map, in world units.
const WATER_HEIGHT: f32 = 60.0;
// Height a wave climbing the glass adds, as a fraction of `WATER_HEIGHT`.
const WATER_SURGE: f32 = 0.6;

// Wave schedule. Times in seconds, lengths in world units.
const IMPACT_PERIOD: f32 = 2.2;
// Shortest stretch length. The stretches fit a whole number of times into the glass.
const HIT_SPACING: f32 = 90.0;
const HIT_WIDTH: f32 = 140.0;
const HIT_CHANCE: f32 = 0.1;
// Fraction of a wave's cycle spent running in; it crashes for the rest.
const ARRIVE: f32 = 0.5;
// Fraction of the crash at which the water stands highest on the glass.
const HIT_PEAK: f32 = 0.2;
// Swell energy: feature size along the glass and drift speed.
const SET_SCALE: f32 = 500.0;
const SET_SPEED: f32 = 0.06;
// Stretches to either side whose waves can reach a point: ceil(0.9 * HIT_WIDTH / HIT_SPACING + 0.3),
// with the shortest stretch length.
const HIT_REACH: i32 = 2;
const HIT_SALT: u32 = 0x1f83d9abu;

// How many times a repeat of about this period fits into the wrap.
fn wrap_count(period: f32) -> f32 {
    return max(floor(TIME_WRAP / period + 0.5), 1.0);
}

// The repeat period nearest to `period` that fits a whole number of times into the wrap.
fn wrap_period(period: f32) -> f32 {
    return TIME_WRAP / wrap_count(period);
}

// A whole-number index folded into 0..count-1. Exact where a plain modulo can land on count itself.
fn wrap_index(index: f32, count: f32) -> f32 {
    return index - count * floor((index + 0.5) / count);
}

// How far the crossfade into the wrap has come: 0 until `WRAP_FADE` seconds before it, then up to 1.
fn wrap_fade(t: f32) -> f32 {
    return smoothstep(TIME_WRAP - WRAP_FADE, TIME_WRAP, t);
}

// Crossfades two samples of 0..1 noise, keeping the noise's spread for uncorrelated samples.
fn wrap_mix(now: f32, wrapped: f32, fade: f32) -> f32 {
    return 0.5 + (mix(now, wrapped, fade) - 0.5) / sqrt(fade * fade + (1.0 - fade) * (1.0 - fade));
}

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

// Value noise along the glass, seamless where the coordinate wraps. `scale` is the feature size in
// world units.
fn wall_noise(wall: vec2<f32>, scale: f32, z: f32) -> f32 {
    let angle = TAU * wall.x / wall.y;
    let ring = wall.y / (TAU * scale);
    return value_noise_2d(vec2<f32>(cos(angle), sin(angle)) * ring + vec2<f32>(z, 0.0));
}

// One wave as it affects a point of the glass.
struct Hit {
    // 1 in the middle of the wave's span, 0 past its ends.
    envelope: f32,
    // The wave's size, 0..1.
    strength: f32,
    // Envelope times strength; 0 when the stretch has no wave now.
    weight: f32,
    // 0..1 while the wave runs in, -1 after.
    incoming: f32,
    // 0..1 from the crash until the wave dies, -1 before.
    crashed: f32,
    // How far the water climbs the glass now, 0..1.
    rise: f32,
    // The span along the glass.
    centre: f32,
    half_width: f32,
    seed: f32,
};

// The current wave of the stretch `k` stretches away from the point's own.
fn stretch_hit(wall: vec2<f32>, t: f32, k: i32) -> Hit {
    let none = Hit(0.0, 0.0, 0.0, -1.0, -1.0, 0.0, 0.0, 1.0, 0.0);
    let stretches = max(floor(wall.y / HIT_SPACING), 1.0);
    let spacing = wall.y / stretches;
    let base = floor(wall.x / spacing);
    let stretch = floored_mod(base + f32(k), stretches);
    let h0 = hash_coords(vec2<f32>(stretch, 0.0), HIT_SALT);
    let cycles = wrap_count(IMPACT_PERIOD * (0.6 + 0.8 * hash_unit(h0)));
    let period = TIME_WRAP / cycles;
    let offset = hash_unit(hash_mix(h0 ^ HASH_GOLDEN));
    let cycle = t / period + offset;
    // The generation counts around the wrap, so the wave in flight at the wrap carries on unchanged.
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
