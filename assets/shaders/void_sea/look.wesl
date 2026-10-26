import package::shaders::common::core::TAU;
import package::shaders::common::gradient_noise::gradient_noise_2d;
import package::shaders::common::hash::{hash_coords, hash_mix, hash_unit};
import package::shaders::common::map_light::{MAP_SUN_ELEVATION_TANGENT, MAP_SUN_GROUND_DIRECTION};
import package::shaders::common::value_noise::value_noise_2d;
import package::shaders::void_sea::waves::{
    HIT_REACH, TIME_WRAP, WATER_HEIGHT, floored_mod, map_size, stretch_hit, wall_coordinate, wall_noise,
    wall_point, wrap_count, wrap_fade, wrap_index, wrap_mix, wrap_period,
};

// The sea's look: a black mercury surface with thin cold streaks, foam where waves meet the glass, and
// the water's face seen through the glass. Lengths are in world units and times in seconds.

// Surface tones: void, body and light patches of slow noise, stretched along the crests.
const VOID_COLOR: vec3<f32> = vec3<f32>(0.00439, 0.00518, 0.00651); // near black
const BODY_COLOR: vec3<f32> = vec3<f32>(0.00750, 0.00913, 0.01161); // dark grey
const LIGHT_COLOR: vec3<f32> = vec3<f32>(0.01161, 0.01444, 0.01850); // slate
const TONE_SCALE: f32 = 260.0;
const TONE_SPEED: f32 = 0.02;
const TONE_STRETCH: f32 = 3.0;
const TONE_LOW: f32 = 0.42;
const TONE_HIGH: f32 = 0.66;
// Unit direction the sea's crests travel in.
const TRAVEL: vec2<f32> = vec2<f32>(0.0, -1.0);

// Toward the horizon every colour closes in on this one, past a clear band around the glass.
const DEEP_COLOR: vec3<f32> = vec3<f32>(0.00212, 0.00243, 0.00304); // black
const HORIZON_START: f32 = 500.0;
// Distance over which colours halve their difference to `DEEP_COLOR`.
const HORIZON_HALF: f32 = 500.0;

// Glass streaks: contour lines of slowly flowing noise, where light catches ripples.
const STROKE_COLOR: vec3<f32> = vec3<f32>(0.64448, 0.69387, 0.75294); // pale steel
const STREAK_SCALE: f32 = 200.0;
const STREAK_STRETCH: f32 = 6.0;
const STREAK_LEVELS: f32 = 3.0;
const STREAK_WIDTH: f32 = 0.9;
const STREAK_SPEED: f32 = 0.08;
const STREAK_BREAK: f32 = 0.68;
// Distance from the glass within which the streaks give way to the incoming waves.
const GLINT_CLEAR: f32 = 180.0;

// Foam of the waves and crashes.
const FOAM_COLOR: vec3<f32> = vec3<f32>(0.71569, 0.76052, 0.81485); // pale grey
const FOAM_CORE_COLOR: vec3<f32> = vec3<f32>(1.0, 1.0, 1.0); // white
const APPROACH_REACH: f32 = 170.0;
const ROLLER_WIDTH: f32 = 18.0;
const ROLLER_BOW: f32 = 30.0;
const WAVE_GLINT_WIDTH: f32 = 0.8;
const BURST_WIDTH: f32 = 24.0;
// Fractions of the crash over which its foam tears apart.
const BREAK_START: f32 = 0.15;
const BREAK_END: f32 = 0.8;
const BLOB_SCALE: f32 = 5.0;
const BLOB_NOISE: f32 = 0.9;
const CORE_BAND: f32 = 0.3;
// Foam always pressed on the glass, and how much each lap swells it.
const CLING: f32 = 3.0;
const LAP_SWELL: f32 = 2.0;
// Tiny drops hopping off the glass at each lap.
const LAP_DROP_SPACING: f32 = 26.0;
const LAP_DROP_CHANCE: f32 = 0.35;
const LAP_DROP_SIZE: f32 = 1.1;
const LAP_DROP_OUT: f32 = 6.0;
// Waves, crashes, echoes and lapping drops all stay within this distance of the glass:
// max(APPROACH_REACH + ROLLER_BOW + ROLLER_WIDTH, CLING + ECHO_REACH + RIPPLE_WIDTH, BURST_WIDTH) + 8.
const FOAM_REACH: f32 = 226.0;

// Lapping rings leaving the glass, and the echo a crash sends back out. The ring period is also the
// rhythm of the lapping.
const RIPPLE_STRENGTH: f32 = 0.09;
const RIPPLE_SPACING: f32 = 22.0;
const RIPPLE_PERIOD: f32 = 2.2;
const RIPPLE_REACH: f32 = 60.0;
const RIPPLE_WIDTH: f32 = 1.0;
const ECHO_STRENGTH: f32 = 0.8;
const ECHO_REACH: f32 = 80.0;

// Charged glass: the glass glows for a moment where a wave crashes. HDR, so it blooms.
const FLASH_COLOR: vec3<f32> = vec3<f32>(0.21223, 0.80695, 1.0); // cyan
const BARRIER_FLASH: f32 = 1.4;
const FLASH_DECAY: f32 = 9.0;

// The water's face against the glass.
const FACE_DEEP_COLOR: vec3<f32> = vec3<f32>(0.00152, 0.00182, 0.00212); // black
const FACE_TOP_COLOR: vec3<f32> = vec3<f32>(0.01229, 0.01521, 0.01938); // slate
const MENISCUS_COLOR: vec3<f32> = vec3<f32>(1.0, 1.0, 1.0); // white
const MENISCUS_WIDTH: f32 = 1.5;
const FACE_FOAM: f32 = 1.5;
const CHURN_DEPTH: f32 = 32.0;
const BUBBLE_SCALE: f32 = 4.0;

// Spray thrown up the glass by crashes.
const DROP_COUNT: f32 = 14.0;
const DROP_SIZE: f32 = 2.0;
const DROP_HEIGHT: f32 = 80.0;
const DROP_OUT: f32 = 8.0;
// How dark a drop's shadow lies on the surface.
const SEA_SHADOW: f32 = 0.55;

const LAP_SALT: u32 = 0x6a09e667u;

// Coverage of a stroke of the given half-width. Strokes thinner than a pixel keep a pixel's width and
// fade instead.
fn ink(distance: f32, half_width: f32, pixel: f32) -> f32 {
    let w = max(half_width, 0.5 * pixel);
    return (1.0 - smoothstep(w - 0.5 * pixel, w + 0.5 * pixel, distance)) * min(half_width / w, 1.0);
}

fn streak_noise(q: vec2<f32>, t: f32) -> f32 {
    let flow = vec2<f32>(0.3, -1.0) * t * STREAK_SPEED;
    return gradient_noise_2d(q + flow) + 0.5 * (gradient_noise_2d(q * 2.1 + vec2<f32>(3.3, 7.1) - flow * 1.3) - 0.5);
}

// Thin cold streaks where light catches ripples on the surface. They follow contour lines of slowly
// flowing noise, stretched along the crests.
fn glass_streaks(p: vec2<f32>, t: f32, pixel: f32) -> f32 {
    let along_crest = vec2<f32>(-TRAVEL.y, TRAVEL.x);
    let q = vec2<f32>(dot(p, along_crest) / STREAK_STRETCH, dot(p, TRAVEL)) / STREAK_SCALE;
    var n = streak_noise(q, t);
    let break_q = q * vec2<f32>(4.0, 1.5);
    var breaks = value_noise_2d(break_q + vec2<f32>(t * 0.1, 0.0));
    let fade = wrap_fade(t);
    if fade > 0.0 {
        n = wrap_mix(n, streak_noise(q, t - TIME_WRAP), fade);
        breaks = wrap_mix(breaks, value_noise_2d(break_q + vec2<f32>((t - TIME_WRAP) * 0.1, 0.0)), fade);
    }
    let level = n * STREAK_LEVELS;
    let to_level = abs(level - floor(level + 0.5));
    let world_distance = to_level / max(fwidth(level), 1e-6) * pixel;
    let unbroken = smoothstep(STREAK_BREAK - 0.06, STREAK_BREAK + 0.06, breaks);
    return ink(world_distance, STREAK_WIDTH, pixel) * unbroken;
}

// The glass's constant small lapping, in time with the rings: 0..1, peaking as a ring leaves the glass.
fn lap_pulse(t: f32) -> f32 {
    return pow(0.5 + 0.5 * cos(TAU * t / wrap_period(RIPPLE_PERIOD)), 8.0);
}

// The lap's number, counted around the wrap.
fn lap_index(t: f32) -> f32 {
    return wrap_index(floor(t / wrap_period(RIPPLE_PERIOD) + 0.5), wrap_count(RIPPLE_PERIOD));
}

// How strong the current lap is at a point. It varies along the glass and from lap to lap.
fn lap_amount(wall: vec2<f32>, t: f32) -> f32 {
    return wall_noise(wall, 60.0, lap_index(t) * 3.7);
}

// Tiny drops hopping off the glass at each lap and falling back.
fn lap_drops(wall: vec2<f32>, d: f32, t: f32, pixel: f32) -> f32 {
    let a = (fract(t / wrap_period(RIPPLE_PERIOD) + 0.5) - 0.5) / 0.35;
    if a <= 0.0 || a >= 1.0 {
        return 0.0;
    }
    let lap = lap_index(t);
    let chance = LAP_DROP_CHANCE * lap_amount(wall, t);
    let slots = max(floor(wall.y / LAP_DROP_SPACING), 1.0);
    let spacing = wall.y / slots;
    let base = floor(wall.x / spacing);
    var cover = 0.0;
    for (var k = -1; k <= 1; k++) {
        let slot = base + f32(k);
        let h = hash_coords(vec2<f32>(floored_mod(slot, slots), lap), LAP_SALT);
        if hash_unit(h) > chance {
            continue;
        }
        let along = (slot + 0.2 + 0.6 * hash_unit(hash_mix(h + 1u))) * spacing;
        let out_from_glass = CLING + LAP_DROP_OUT * hash_unit(hash_mix(h + 2u)) * 4.0 * a * (1.0 - a);
        let radius = LAP_DROP_SIZE * (0.6 + 0.8 * hash_unit(hash_mix(h + 3u))) * (1.0 - 0.6 * a);
        cover = max(cover, ink(length(vec2<f32>(wall.x - along, d - out_from_glass)), radius, pixel));
    }
    return cover;
}

struct Foam {
    foam: f32,
    // The white core of the foam.
    core: f32,
    // Glints on incoming crests and crash echoes.
    lines: f32,
    // The crash flash on the glass.
    flash: f32,
};

// Waves on the sea's surface near the glass. A wave is born out at sea as a faint glint on the swell and
// runs in at a steady speed, its middle ahead of its ends; big waves start further out. It gathers foam
// as it nears the glass, flecks first and then a solid cap. It crashes into the glass, where its foam
// bursts against the wall, tears apart and dies in place, and its echo can run back out. A little water
// always stays pressed on the glass and swells with each lap. Every wave reaching the point adds its part.
fn impact_foam(p: vec2<f32>, d: f32, t: f32, pixel: f32) -> Foam {
    let wall = wall_coordinate(p);
    var ragged = wall_noise(wall, 14.0, t * 0.9);
    let fade = wrap_fade(t);
    if fade > 0.0 {
        ragged = wrap_mix(ragged, wall_noise(wall, 14.0, (t - TIME_WRAP) * 0.9), fade);
    }
    let cling = CLING * (1.0 + 0.6 * (ragged - 0.5)) + LAP_SWELL * lap_pulse(t) * lap_amount(wall, t);
    var result = Foam(
        1.0 - smoothstep(cling - 0.5 * pixel, cling + 0.5 * pixel, d),
        1.0 - smoothstep(0.5 * cling - 0.5 * pixel, 0.5 * cling + 0.5 * pixel, d),
        0.0,
        0.0,
    );
    result.foam = max(result.foam, lap_drops(wall, d, t, pixel));

    // Edge softness from the slopes of the shapes and the noise, so it needs no screen derivatives.
    // Capped so a shape thinner than a pixel fades out instead of smearing.
    let noise_slope = BLOB_NOISE / BLOB_SCALE;
    for (var k = -HIT_REACH; k <= HIT_REACH; k++) {
        let hit = stretch_hit(wall, t, k);
        if hit.weight <= 0.01 {
            continue;
        }
        let q = p / BLOB_SCALE + vec2<f32>(hit.seed * 17.3, hit.seed * 5.9);
        let n = 0.65 * value_noise_2d(q) + 0.35 * value_noise_2d(q * 2.3 + vec2<f32>(5.1, 1.7));
        if hit.incoming >= 0.0 {
            let coming = hit.incoming;
            let front = APPROACH_REACH * (0.35 + 0.65 * hit.strength) * (1.0 - coming) + (1.0 - hit.envelope) * ROLLER_BOW * hit.strength;
            // The glint on the crest, fading in as the wave is born.
            let born = smoothstep(0.0, 0.2, coming) * hit.envelope;
            result.lines = max(result.lines, ink(abs(d - front), WAVE_GLINT_WIDTH * (0.5 + 0.5 * hit.strength), pixel) * born * (0.4 + 0.6 * hit.strength));
            // The foam cap: a band from the crest back, fading out toward the ends of its span.
            let thickness = max(ROLLER_WIDTH * coming * hit.weight, 0.5);
            let gathered = smoothstep(0.15, 0.85, coming);
            let roller = 1.0 - abs((d - front) / thickness - 0.5) * 2.0 + (n - 0.5) * BLOB_NOISE - (1.0 - gathered) * 1.2 - (1.0 - hit.envelope) * 1.5;
            let aa = min(pixel * (2.0 / thickness + noise_slope), 0.2);
            result.foam = max(result.foam, smoothstep(-aa, aa, roller));
            result.core = max(result.core, smoothstep(0.45 - aa, 0.45 + aa, roller) * coming * hit.strength);
        } else {
            // The crash: a mass of foam against the glass that tears and dissolves without moving off it.
            let struck = hit.crashed;
            let reach_out = max(BURST_WIDTH * hit.weight * smoothstep(0.0, 0.06, struck), 0.5);
            let crash = max(1.0 - d / reach_out, -1.0) + (n - 0.5) * BLOB_NOISE;
            // Rises past the densest fragment by the end, so every fragment dissolves before the next wave.
            let threshold = mix(-0.15, 1.05 + 0.5 * BLOB_NOISE, smoothstep(BREAK_START, BREAK_END, struck));
            let aa = min(pixel * (1.0 / reach_out + noise_slope), 0.2);
            result.foam = max(result.foam, smoothstep(threshold - aa, threshold + aa, crash));
            result.core = max(result.core, smoothstep(threshold + CORE_BAND - aa, threshold + CORE_BAND + aa, crash));
            // The crash's echo: a ripple running back out from its span, as strong as the crash.
            let echo = CLING + struck * ECHO_REACH;
            result.lines = max(result.lines, ink(abs(d - echo), RIPPLE_WIDTH * (1.0 - struck), pixel) * hit.weight * (1.0 - struck) * ECHO_STRENGTH);
            result.flash = max(result.flash, hit.weight * exp(-struck * FLASH_DECAY));
        }
    }
    return result;
}

// The slow tone patches drifting across the sea.
fn tone_noise(tone_p: vec2<f32>, t: f32) -> f32 {
    let drift = vec2<f32>(t * TONE_SPEED, -0.6 * t * TONE_SPEED);
    return 0.65 * gradient_noise_2d(tone_p + drift) + 0.35 * gradient_noise_2d(tone_p * 2.3 - drift * 1.7);
}

// The sea's surface at the surface point `p`: tones, streaks, rings and the waves near the glass.
// `d` is the distance from the glass and `pixel` the surface's world size of a screen pixel.
fn sea_surface(p: vec2<f32>, d: f32, t: f32, pixel: f32) -> vec3<f32> {
    // Three flat tones from slow noise, their shapes stretched along the crests.
    let along_crest = vec2<f32>(-TRAVEL.y, TRAVEL.x);
    let tone_p = vec2<f32>(dot(p, along_crest) / TONE_STRETCH, dot(p, TRAVEL)) / TONE_SCALE;
    var tone = tone_noise(tone_p, t);
    let fade = wrap_fade(t);
    if fade > 0.0 {
        tone = wrap_mix(tone, tone_noise(tone_p, t - TIME_WRAP), fade);
    }
    let tone_aa = fwidth(tone);
    var color = mix(VOID_COLOR, BODY_COLOR, smoothstep(TONE_LOW - tone_aa, TONE_LOW + tone_aa, tone));
    color = mix(color, LIGHT_COLOR, smoothstep(TONE_HIGH - tone_aa, TONE_HIGH + tone_aa, tone));

    // The open sea's streaks. Near the glass the incoming waves take over.
    let open_sea = smoothstep(0.5 * GLINT_CLEAR, GLINT_CLEAR, d);
    color = mix(color, STROKE_COLOR, glass_streaks(p, t, pixel) * open_sea);

    // Faint lapping rings leaving the glass; crash echoes come with the waves.
    let ripple_distance = d - CLING;
    if ripple_distance > 0.0 && ripple_distance < RIPPLE_REACH {
        let phase = ripple_distance / RIPPLE_SPACING - t / wrap_period(RIPPLE_PERIOD);
        let to_line = abs(fract(phase + 0.5) - 0.5) * RIPPLE_SPACING;
        // The ring's number, counted around the wrap.
        let ring = wrap_index(floor(phase + 0.5), wrap_count(RIPPLE_PERIOD));
        let closest = clamp(p, vec2<f32>(0.0), map_size());
        let broken = step(0.5, gradient_noise_2d(closest / 57.0 + vec2<f32>(ring * 3.1, 0.0)));
        let ring_fade = 1.0 - ripple_distance / RIPPLE_REACH;
        color = mix(color, STROKE_COLOR, ink(to_line, RIPPLE_WIDTH * ring_fade, pixel) * broken * RIPPLE_STRENGTH);
    }

    if d < FOAM_REACH {
        let foam = impact_foam(p, d, t, pixel);
        color = mix(color, STROKE_COLOR, foam.lines);
        color = mix(color, FOAM_COLOR, foam.foam);
        color = mix(color, FOAM_CORE_COLOR, foam.core);
        // Charged glass: the crash flash glows along the glass.
        color += FLASH_COLOR * BARRIER_FLASH * foam.flash * exp(-d / 4.0);
    }
    // Past a clear band around the glass, every colour closes in on the deep at the same rate toward the
    // horizon, so they keep their contrast with each other while they darken, until they merge.
    let horizon = exp2(-max(d - HORIZON_START, 0.0) / HORIZON_HALF);
    return DEEP_COLOR + (color - DEEP_COLOR) * horizon;
}

// Where the line of sight from a ground point to the sea-surface point above it crosses the glass.
struct GlassCrossing {
    // 0 at the ground, 1 at the sea's top.
    height_fraction: f32,
    // Depth below the sea's top.
    below_surface: f32,
    // Position along the glass.
    wall: vec2<f32>,
};

fn glass_crossing(p: vec2<f32>, surface: vec2<f32>, sea_height: f32) -> GlassCrossing {
    let size = map_size();
    let sight = surface - p;
    var enter = 0.0;
    if p.x < 0.0 && abs(sight.x) > 1e-5 {
        enter = max(enter, -p.x / sight.x);
    }
    if p.x > size.x && abs(sight.x) > 1e-5 {
        enter = max(enter, (size.x - p.x) / sight.x);
    }
    if p.y < 0.0 && abs(sight.y) > 1e-5 {
        enter = max(enter, -p.y / sight.y);
    }
    if p.y > size.y && abs(sight.y) > 1e-5 {
        enter = max(enter, (size.y - p.y) / sight.y);
    }
    let height_fraction = clamp(enter, 0.0, 1.0);
    return GlassCrossing(height_fraction, sea_height * (1.0 - height_fraction), wall_coordinate(mix(p, surface, height_fraction)));
}

// Bubbles rising through the churn on the glass.
fn churn_noise(wall: vec2<f32>, below_surface: f32, t: f32) -> f32 {
    let q = vec2<f32>(wall.x, below_surface + t * 6.0) / BUBBLE_SCALE;
    return 0.65 * value_noise_2d(q) + 0.35 * value_noise_2d(q * 2.3 + vec2<f32>(5.1, 1.7));
}

// The sea's face against the glass, seen between the map's edge and the rim of the raised surface. The
// water is darker deeper down. At the top, foam presses on the glass and swells with each lap, and
// crashes drive white churn down into the water. `below_width` and `along_width` are the crossing's
// change across a pixel, taken where every pixel runs.
fn glass_face(crossing: GlassCrossing, below_width: f32, along_width: f32, t: f32) -> vec3<f32> {
    let height_fraction = crossing.height_fraction;
    let below_surface = crossing.below_surface;
    let wall = crossing.wall;
    var color = mix(FACE_DEEP_COLOR, FACE_TOP_COLOR, height_fraction * height_fraction);
    // A thin cold line where the water's top meets the glass.
    color = mix(color, MENISCUS_COLOR, 1.0 - smoothstep(0.0, MENISCUS_WIDTH, below_surface));

    var churn_depth = FACE_FOAM + LAP_SWELL * lap_pulse(t) * lap_amount(wall, t);
    // Charged glass: the glass shows itself for a moment where a wave crashes.
    var flash = 0.0;
    for (var k = -HIT_REACH; k <= HIT_REACH; k++) {
        let hit = stretch_hit(wall, t, k);
        if hit.crashed < 0.0 {
            continue;
        }
        churn_depth = max(churn_depth, CHURN_DEPTH * hit.weight * smoothstep(0.0, 0.08, hit.crashed) * pow(1.0 - hit.crashed, 1.5));
        flash = max(flash, hit.weight * exp(-hit.crashed * FLASH_DECAY));
    }
    color += FLASH_COLOR * BARRIER_FLASH * flash * (0.25 + 0.75 * height_fraction);
    var n = churn_noise(wall, below_surface, t);
    let fade = wrap_fade(t);
    if fade > 0.0 {
        n = wrap_mix(n, churn_noise(wall, below_surface, t - TIME_WRAP), fade);
    }
    let churn = 1.0 - below_surface / max(churn_depth, 1e-3) + (n - 0.5) * BLOB_NOISE;
    // The churn's change across a pixel: its depth falloff and its noise.
    let aa = clamp(below_width / max(churn_depth, 1e-3) + BLOB_NOISE * (along_width + below_width) / BUBBLE_SCALE, 1e-4, 0.2);
    color = mix(color, FOAM_COLOR, smoothstep(-aa, aa, churn));
    color = mix(color, FOAM_CORE_COLOR, smoothstep(0.45 - aa, 0.45 + aa, churn));
    // A dark crease where the water meets the map's ground.
    return color * mix(0.35, 1.0, smoothstep(0.0, 0.12, height_fraction));
}

struct Spray {
    // Drop coverage.
    cover: f32,
    // Coverage of the drops' shadows on the surface.
    shadow: f32,
};

// Spray thrown up the glass by crashes, more and higher from bigger waves. Each drop rises above the sea
// and falls back near the glass. The perspective that lifts the sea carries a drop further from the
// camera the higher it is, and its shadow lies on the sea's surface. `p` is the ground point under the
// pixel; a ground point at height h shows at `camera + (p - camera) * camera_height / (camera_height - h)`.
fn spray_drops(p: vec2<f32>, camera: vec2<f32>, camera_height: f32, t: f32, pixel: f32) -> Spray {
    let wall = wall_coordinate(p);
    let sun_away = -MAP_SUN_GROUND_DIRECTION / MAP_SUN_ELEVATION_TANGENT;
    let calm_lift = camera_height / (camera_height - WATER_HEIGHT);
    var result = Spray(0.0, 0.0);
    for (var k = -HIT_REACH; k <= HIT_REACH; k++) {
        let hit = stretch_hit(wall, t, k);
        if hit.crashed < 0.0 {
            continue;
        }
        let drops = DROP_COUNT * pow(hit.strength, 1.5);
        for (var i = 0; f32(i) < drops; i++) {
            let hd = hash_mix(bitcast<u32>(hit.seed) ^ (u32(i) * 0x2545f491u));
            let launch = 0.02 + 0.2 * hash_unit(hd);
            let flight = 0.35 + 0.35 * hash_unit(hash_mix(hd + 1u));
            let a = (hit.crashed - launch) / flight;
            if a <= 0.0 || a >= 1.0 {
                continue;
            }
            let start = wall_point(hit.centre + hit.half_width * 1.6 * (hash_unit(hash_mix(hd + 2u)) - 0.5));
            let base = start.position + start.outward * (CLING + DROP_OUT * hash_unit(hash_mix(hd + 3u)) * a);
            let above_sea = 4.0 * a * (1.0 - a) * DROP_HEIGHT * hit.strength * (0.6 + 0.8 * hash_unit(hash_mix(hd + 4u)));
            let lift = camera_height / (camera_height - WATER_HEIGHT - above_sea);
            let size = DROP_SIZE * (0.6 + 0.8 * hash_unit(hash_mix(hd + 5u))) * lift;
            result.cover = max(result.cover, ink(length(p - (camera + (base - camera) * lift)), size, pixel));
            let on_surface = base + sun_away * above_sea;
            result.shadow = max(result.shadow, ink(length(p - (camera + (on_surface - camera) * calm_lift)), size, pixel));
        }
    }
    return result;
}
