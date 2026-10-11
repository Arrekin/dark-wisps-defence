import bevy_sprite_render::mesh2d::vertex_output::VertexOutput;
import bevy_sprite_render::mesh2d::view_bindings::globals;
import package::shaders::common::core::PI;
import package::shaders::common::fire_ramp::fire_ramp;
import package::shaders::common::gradient_noise::{gradient_fbm_2d, gradient_noise_2d};
import package::shaders::common::hash::{hash_coords, hash_mix, hash_unit};

// Flamethrower stream: a pressurized jet of fuel that ignites and loses its shape in billows.
//
// UV x runs from the nozzle (0) to the far end of the range (1). Two layers make up the stream:
// - **Jet** - the ejection. A thin, rigid spear with a fixed taper, crisp edges and streaked
//   texture. It leaves the nozzle as a faint unlit stream and ignites over a short zone.
// - **Cloud** - takes over from the jet. It has no outline of its own: a noise density field is
//   filled wherever it is dense enough, inside a wide soft envelope that only biases density
//   toward the axis. Noise cells grow with distance, so billows grow as they travel. Past a
//   ragged burnout front the fill threshold rises, so the cloud breaks into clumps.
// The cloud churns as it travels: its swirl field moves independently of the density, so
// billows fold and reshape instead of sliding. Pressure surges travel down the stream, swelling
// density and heat, and embers spark off the billows' fringes.
// Heat follows density: dense cores burn yellow-orange, and the band just below the fill
// threshold cools into translucent smoky red, giving every billow a red fringe. Ridged veins
// crack the fire with brighter streaks. Heat stays in the yellow-orange band of the fire-wisp
// palette, never reaching white. The mesh is wider than the stream, leaving room for the
// billows. Nothing is sampled from a texture.
//
// The stream ends just past its front, which travels out from the nozzle and stops at whatever
// it hits. At a hit it splashes: the cloud takes over even in jet territory, the envelope widens
// so the fire wraps around the wisp, billows grow to the splash's size, and density and heat rise.
// The splash's force comes from where it lands: a hit close to the nozzle takes the stream at full
// pressure and bursts widely, while a hit near the end barely adds to what is already billowing.
// Force scales the splash's width, its reach past the wisp, and its strength.

// Field order and types mirror `FlameUniform` in buildings_internal/src/towers/blaster/tower_flamethrower.rs.
struct Flame {
    intensity: f32,         // 0 = extinguished, 1 = full stream
    seed: f32,              // per-tower phase offset
    start_half_width: f32,  // stream half-width at the nozzle, relative to the mesh width
    end_half_width: f32,    // cone half-width at the far end, relative to the mesh width
    aspect: f32,            // world length / world mesh width, keeps noise proportional
    front: f32,             // where the stream ends, before its burnout; 1 = full range
    splash_radius: f32,     // splash reach along the stream at full force, relative to the stream length
    splash: f32,            // 0 = no hit, 1 = full splash at the front
    splash_half_width: f32, // width the splash adds on each side at full force, relative to the mesh width
};

@group(2) @binding(0)
var<uniform> flame: Flame;

// ── Jet ──────────────────────────────────────────────────────────────────────
const JET_END_WIDTH: f32 = 0.35;     // jet half-width at the far end, relative to the cone's end half-width
const JET_TEAR: f32 = 0.15;
const JET_SOFTNESS: f32 = 0.35;
const STREAK_FLOW: f32 = 18.0;       // jet noise scroll, in mesh widths per second
const STREAK: f32 = 0.15;            // < 1 stretches jet noise along the flow
const IGNITE_START: f32 = 0.06;
const IGNITE_END: f32 = 0.2;
const FUEL_HEAT: f32 = 0.4;          // dull orange of the unlit fuel
const FUEL_ALPHA: f32 = 0.3;

// ── Handover from jet to cloud ───────────────────────────────────────────────
const BILLOW_START: f32 = 0.18;
const BILLOW_END: f32 = 0.5;

// ── Cloud ────────────────────────────────────────────────────────────────────
const ENVELOPE_WIDTH: f32 = 1.5;     // envelope half-width at the far end, relative to the cone's end half-width
const ENVELOPE_GROWTH: f32 = 1.3;    // > 1 makes the envelope widen faster further out
const ENVELOPE_BIAS: f32 = 0.9;      // how strongly the envelope pulls density toward the axis
const ENVELOPE_LIMIT_START: f32 = 1.2; // beyond this multiple of the envelope half-width the cloud fades out
const ENVELOPE_LIMIT_END: f32 = 1.6;   // the cloud is gone at this multiple, however dense the noise
const BLOB_NEAR: f32 = 0.08;         // noise cell width across the stream at the nozzle, in mesh widths
const BLOB_NEAR_ALONG: f32 = 0.22;   // noise cell length along the stream at the nozzle; longer than wide reads as speed
const BLOB_FAR: f32 = 0.3;           // noise cell size at the far end, in mesh widths; cells are round there
const CLOUD_FLOW: f32 = 7.0;         // cloud scroll, in noise cells per second
const WARP: f32 = 0.9;               // swirl, in noise cells
const CHURN_SPEED: f32 = 2.6;        // swirl field movement relative to the density, in swirl cells per second
const VEIN_WARP: f32 = 0.3;          // share of the swirl applied to the veins, which smear under full swirl
const DRIFT: f32 = 0.3;              // sideways drift of the far end, relative to the cone's end half-width
const DRIFT_SPEED: f32 = 4.0;
const FILL_THRESHOLD: f32 = 0.5;     // density at which the cloud burns
const FILL_SOFTNESS: f32 = 0.12;
const SMOKE_BAND: f32 = 0.14;        // density range below the fill threshold that shows as smoke

// ── Pressure surges ──────────────────────────────────────────────────────────
const SURGE_FLOW: f32 = 13.0;        // surge travel, in mesh widths per second
const SURGE_LENGTH: f32 = 0.5;       // surges per mesh width of stream
const SURGE_DENSITY: f32 = 0.22;     // density swing of a surge
const SURGE_HEAT: f32 = 0.12;        // heat swing of a surge

// ── Embers ───────────────────────────────────────────────────────────────────
const EMBER_SCALE: f32 = 6.0;        // ember grid cells per mesh width
const EMBER_FLOW: f32 = 15.0;        // ember travel, in grid cells per second
const EMBER_CHANCE: f32 = 0.3;       // share of grid cells holding an ember
const EMBER_RADIUS: f32 = 0.09;      // in grid cells
const EMBER_LIFE: f32 = 2.5;         // ember flashes per second
const EMBER_HEAT: f32 = 0.8;
const EMBER_ALPHA: f32 = 0.9;
const EMBER_SALT: u32 = 0x5bd1e995u;

// ── Burnout ──────────────────────────────────────────────────────────────────
const BURNOUT_START: f32 = 0.7;      // where the burnout front sits, before jitter
const BURNOUT_JITTER: f32 = 0.3;     // how much earlier dense billows pull the front; never later, so x = 1 is always burnt out
const BURNOUT_DEPTH: f32 = 0.35;     // fill threshold rise once fully burnt out

// ── Splash ───────────────────────────────────────────────────────────────────
const SPLASH_FALLOFF: f32 = 1.5;     // > 1 tightens the splash around the impact point
const SPLASH_END_FADE: f32 = 0.85;   // share of the splash's force lost by the far end of the stream
const SPLASH_SPILL: f32 = 0.8;       // share of the splash reach that continues past the impact point
const SPLASH_BURNOUT: f32 = 0.55;    // share of the splash reach over which the spill burns out
const SPLASH_CELL: f32 = 1.0;        // splash billow size, relative to the width the splash adds
const SPLASH_DENSITY: f32 = 0.25;
const SPLASH_HEAT: f32 = 0.1;

// ── Heat (input to `fire_ramp`) ──────────────────────────────────────────────
const HEAT_CORE: f32 = 0.76;         // yellow jet core
const HEAT_BODY: f32 = 0.6;          // orange body
const HEAT_SMOKE: f32 = 0.22;        // dark red smoke
const DENSITY_HEAT: f32 = 0.6;       // heat added per unit of density above the fill threshold
const VEIN_HEAT: f32 = 0.3;          // heat swing between veins and gaps
const VEIN_ALPHA: f32 = 0.6;         // opacity swing between veins and gaps
const MAX_HEAT: f32 = 0.84;          // just under where the ramp turns white
const SMOKE_ALPHA: f32 = 0.35;
const MAX_ALPHA: f32 = 0.97;

@fragment
fn fragment(mesh: VertexOutput) -> @location(0) vec4<f32> {
    let x = mesh.uv.x;
    let time = globals.time + flame.seed;

    let force = 1.0 - SPLASH_END_FADE * flame.front;
    let splash_radius = max(flame.splash_radius * force, 0.001);
    let to_impact = (x - flame.front) / splash_radius;
    let impact_zone = exp(-to_impact * to_impact * SPLASH_FALLOFF) * flame.splash;
    let splash = impact_zone * force;
    let billow = max(smoothstep(BILLOW_START, BILLOW_END, x), splash);

    let envelope_growth = pow(x, ENVELOPE_GROWTH);
    let drift = (gradient_noise_2d(vec2<f32>(time * DRIFT_SPEED, flame.seed)) - 0.5) * 2.0;
    let y = mesh.uv.y - 0.5 - drift * DRIFT * flame.end_half_width * envelope_growth;

    // Jet: fixed taper, crisp torn edges, streaks along the flow.
    let jet_half_width = mix(flame.start_half_width, flame.end_half_width * JET_END_WIDTH, x);
    let jet_across = y / max(jet_half_width, 0.001);
    let streak_point = vec2<f32>((x * flame.aspect - time * STREAK_FLOW) * STREAK, jet_across * 0.8);
    let streaks = gradient_fbm_2d(streak_point, 3);
    let jet_radial = abs(jet_across) + (streaks - 0.5) * JET_TEAR;
    let jet = smoothstep(1.0, 1.0 - JET_SOFTNESS, jet_radial);

    // Cloud space: cells are stretched along the flow near the nozzle and grow round with
    // distance. Integrating along x by the cell length keeps the mapping static, so scrolling
    // never shears it.
    let cell = max(mix(BLOB_NEAR, BLOB_FAR, x), flame.splash_half_width * SPLASH_CELL * splash);
    let cell_along = mix(BLOB_NEAR_ALONG, BLOB_FAR, x);
    let along = flame.aspect * log(cell_along) / (BLOB_FAR - BLOB_NEAR_ALONG);
    let cloud_point = vec2<f32>(along - time * CLOUD_FLOW, y / cell);
    let churn = vec2<f32>(time * CHURN_SPEED * 0.5, time * CHURN_SPEED);
    let swirl = vec2<f32>(
        gradient_fbm_2d(cloud_point * 0.6 + churn + vec2<f32>(5.0, 1.0), 2),
        gradient_fbm_2d(cloud_point * 0.6 - churn + vec2<f32>(-3.0, 9.0), 2),
    ) - 0.5;
    let surge = gradient_noise_2d(vec2<f32>((x * flame.aspect - time * SURGE_FLOW) * SURGE_LENGTH, flame.seed + 3.0)) - 0.5;
    let density = gradient_fbm_2d(cloud_point + swirl * WARP + vec2<f32>(31.0, 7.0), 4) + surge * SURGE_DENSITY;

    // Density biased toward the axis by the envelope and lowered past the burnout front.
    let envelope_half_width = mix(flame.start_half_width, flame.end_half_width * ENVELOPE_WIDTH, envelope_growth) + flame.splash_half_width * splash;
    let envelope_radial = abs(y) / max(envelope_half_width, 0.001);
    let envelope = clamp(1.0 - envelope_radial * envelope_radial, 0.0, 1.0);
    // Without a hit the burnout spans the stream's tail; at a hit it spans the splash past the front.
    let burnout_end = min(1.0, flame.front + splash_radius * SPLASH_SPILL * flame.splash);
    let burnout_length = mix(1.0 - BURNOUT_START, splash_radius * SPLASH_BURNOUT, flame.splash);
    let burnout_jitter = BURNOUT_JITTER * burnout_length / (1.0 - BURNOUT_START);
    let burnout = smoothstep(burnout_end - burnout_length, burnout_end, x + max(density, 0.0) * burnout_jitter);
    let cloud_density = density + ENVELOPE_BIAS * (envelope - 0.5) - BURNOUT_DEPTH * burnout + SPLASH_DENSITY * splash * envelope;
    let envelope_limit = 1.0 - smoothstep(ENVELOPE_LIMIT_START, ENVELOPE_LIMIT_END, envelope_radial);
    let cloud_fire = smoothstep(FILL_THRESHOLD, FILL_THRESHOLD + FILL_SOFTNESS, cloud_density) * envelope_limit;
    let cloud_smoke = smoothstep(FILL_THRESHOLD - SMOKE_BAND, FILL_THRESHOLD, cloud_density) * envelope_limit;

    let fire = mix(jet, cloud_fire, billow);
    let smoke = cloud_smoke * billow;

    // Ridged veins of brighter fire with darker gaps between them.
    let vein_point = cloud_point + swirl * WARP * VEIN_WARP;
    let streak_detail = gradient_fbm_2d(streak_point * 2.5 + vec2<f32>(0.0, 11.0), 3);
    let cloud_detail = gradient_fbm_2d(vein_point * 1.8 + vec2<f32>(-17.0, 4.0), 3);
    let veins = 1.0 - abs(mix(streak_detail, cloud_detail, billow) * 2.0 - 1.0);

    // Jet heat peaks along its centerline; cloud heat follows density.
    let jet_heat = mix(HEAT_BODY, HEAT_CORE, (1.0 - clamp(abs(jet_across), 0.0, 1.0)) * (1.0 - x));
    let cloud_heat = HEAT_BODY + DENSITY_HEAT * max(cloud_density - FILL_THRESHOLD, 0.0) + SPLASH_HEAT * splash;
    let fire_heat = mix(jet_heat, cloud_heat, billow) + VEIN_HEAT * (veins - 0.5) + surge * SURGE_HEAT;
    let burning = fire / max(max(fire, smoke), 0.001);
    // The splash burns even when it lands before the fuel would have ignited.
    let lit = max(smoothstep(IGNITE_START, IGNITE_END, x + (streaks - 0.5) * 0.06), impact_zone);
    let heat = min(mix(FUEL_HEAT, mix(HEAT_SMOKE, fire_heat, burning), lit), MAX_HEAT);

    let fire_alpha = fire * (1.0 - VEIN_ALPHA * (1.0 - veins));
    let body_alpha = max(fire_alpha, smoke * SMOKE_ALPHA);
    let edge_fade = 1.0 - smoothstep(0.42, 0.5, abs(mesh.uv.y - 0.5));
    let alpha = clamp(mix(FUEL_ALPHA * jet, body_alpha, lit) * (1.0 - burnout * burnout) * edge_fade, 0.0, MAX_ALPHA) * flame.intensity;

    // Embers: sparse flashing dots carried by the flow around the billows, not inside them.
    let ember_point = vec2<f32>(x * flame.aspect * EMBER_SCALE - time * EMBER_FLOW, y * EMBER_SCALE);
    let ember_cell = floor(ember_point);
    let ember_hash = hash_coords(ember_cell, EMBER_SALT);
    let ember_center = vec2<f32>(hash_unit(ember_hash), hash_unit(hash_mix(ember_hash))) * 0.6 + 0.2;
    let ember_roll = hash_unit(hash_mix(hash_mix(ember_hash)));
    let ember_flash = sin(fract(time * EMBER_LIFE + ember_roll * 7.0) * PI);
    let ember_exists = step(ember_roll, EMBER_CHANCE);
    let ember_zone = smoothstep(0.05, 0.4, envelope) * (1.0 - cloud_fire) * billow * (1.0 - burnout) * edge_fade;
    let ember = smoothstep(EMBER_RADIUS, 0.0, length(fract(ember_point) - ember_center)) * ember_exists * ember_flash * ember_zone;

    let color = mix(fire_ramp(heat), fire_ramp(EMBER_HEAT), ember);
    return vec4<f32>(color, max(alpha, ember * EMBER_ALPHA * flame.intensity));
}
