import bevy_sprite_render::mesh2d::vertex_output::VertexOutput;
import package::shaders::common::clock::{ClockInstant, game_time, game_time_since};
import package::shaders::common::core::PI;
import package::shaders::common::hash::{hash_mix, hash_unit};
import package::shaders::common::noise::noise_2d;

// Laser beam, burning glass: focused light. The beam itself is a faint thread; the violence is at
// the contact, where a blinding hotspot sits in a plume of plasma and throws sparks back toward the
// tower. The barrel tip glows while firing and cools after.
//
// Coordinates are in world units along the beam (`s`, 0 at the muzzle, `length` at the target) and
// across it (`u`). `LIGHT_COLOR` sets the light's colour; the core leans to it by `CORE_TINT`.
// Additive output.

// Mirrors `LaserBeamLight` in weaponry_internal/src/laser_beam.rs.
struct Instance {
    engaged: ClockInstant,   // the beam locked on
    released: ClockInstant,  // the beam switched off; at or before `engaged` while it still fires
    seed: f32,
    length: f32,             // muzzle to the target
    pad: f32,                // quad overhang past each end
    half_width: f32,         // quad half-width
};

// ── Envelope ─────────────────────────────────────────────────────────────────
const SNAP: f32 = 0.8;            // extra brightness when firing starts
const SNAP_DECAY: f32 = 0.08;     // seconds for the snap to settle
const BARREL_GAIN: f32 = 1.5;     // barrel tip heat glow
const HEAT_UP: f32 = 1.0;         // seconds of firing to heat the barrel fully
const COOL: f32 = 0.8;            // seconds for the barrel to cool to 1/e

// ── Colour ───────────────────────────────────────────────────────────────────
const LIGHT_COLOR: vec3<f32> = vec3<f32>(0.46, 0.34, 1.0);
const CORE_TINT: f32 = 0.15;       // how far the core leans to the light's colour
const HALO_COLOR: vec3<f32> = vec3<f32>(0.85, 0.9, 1.0);

// ── Beam ─────────────────────────────────────────────────────────────────────
const CORE_WIDTH: f32 = 0.9;      // core half-width
const CORE_GAIN: f32 = 1.2;
const RIM_WIDTH: f32 = 1.4;
const RIM_GAIN: f32 = 0.5;
const HALO_WIDTH: f32 = 5.0;
const HALO_GAIN: f32 = 0.1;
const FLOW_GAIN: f32 = 0.2;       // brightness range of the flowing streaks
const FLOW_SPEED: f32 = 900.0;    // world units per second, muzzle to target
const FLOW_SCALE: f32 = 22.0;     // streak length

// ── Contact ──────────────────────────────────────────────────────────────────
const HOT_RADIUS: f32 = 6.0;      // hotspot radius
const HOT_GAIN: f32 = 7.0;
const HOT_FLICKER: f32 = 0.3;     // hotspot brightness variation
const PLUME_RADIUS: f32 = 10.0;   // plasma plume radius
const PLUME_GAIN: f32 = 1.5;
const SPARK_GAIN: f32 = 3.0;      // sparks thrown back off the target
const SPARK_COUNT: i32 = 24;      // sparks alive at once
const SPARK_SPEED: f32 = 200.0;   // world units per second
const SPARK_LIFE: f32 = 0.22;     // seconds a spark lives
const SPARK_SPREAD: f32 = 1.1;    // half-angle of the spray around the reflected beam, radians
const SPARK_STREAK: f32 = 0.025;  // streak length, seconds of travel

fn random(index: u32, salt: u32) -> f32 {
    return hash_unit(hash_mix(index ^ hash_mix(salt)));
}

// Beam state at the current instant.
struct Firing {
    level: f32,  // beam brightness, with the snap
    on: f32,     // 1 while firing, 0 from the instant firing stops
    heat: f32,   // barrel tip heat
    start: f32,  // game time when firing started
};

fn firing_state(beam: Instance) -> Firing {
    let since_engaged = game_time_since(beam.engaged);
    let since_released = game_time_since(beam.released);
    let firing = since_released >= since_engaged;
    let on = select(0.0, 1.0, firing);
    let level = on * (1.0 + SNAP * exp(-since_engaged / SNAP_DECAY));
    let fired = since_engaged - select(since_released, 0.0, firing);
    let heat_peak = clamp(fired / HEAT_UP, 0.0, 1.0);
    let heat = select(heat_peak * exp(-since_released / COOL), heat_peak, firing);
    return Firing(level, on, heat, game_time() - since_engaged);
}

// Fades to zero before the quad's edges so nothing is clipped.
fn edge_window(beam: Instance, s: f32, u: f32) -> f32 {
    let across = 1.0 - smoothstep(0.55, 1.0, abs(u) / beam.half_width);
    let along = smoothstep(-beam.pad, -0.45 * beam.pad, s) * (1.0 - smoothstep(beam.length + 0.45 * beam.pad, beam.length + beam.pad, s));
    return across * along;
}

fn look(mesh: VertexOutput, beam: Instance) -> vec4<f32> {
    let s = mesh.uv.x * (beam.length + 2.0 * beam.pad) - beam.pad;
    let u = (0.5 - mesh.uv.y) * 2.0 * beam.half_width;
    let time = game_time();
    let state = firing_state(beam);
    let ax = abs(u);
    let along = smoothstep(-1.0, 1.0, s) * (1.0 - smoothstep(beam.length - 1.0, beam.length + 1.0, s));

    // Beam: a faint core with a coloured rim and halo.
    let flow = 1.0 + FLOW_GAIN * (noise_2d(vec2<f32>((s - time * FLOW_SPEED) / FLOW_SCALE, u / 4.0 + beam.seed)) - 0.5) * 2.0;
    let core_x = u / CORE_WIDTH;
    var color = mix(vec3<f32>(1.0), LIGHT_COLOR, CORE_TINT) * exp(-core_x * core_x) * CORE_GAIN;
    let rim_x = (ax - CORE_WIDTH) / RIM_WIDTH;
    color += LIGHT_COLOR * exp(-rim_x * rim_x) * RIM_GAIN * flow;
    color += mix(HALO_COLOR, LIGHT_COLOR, 0.5) * exp(-ax / HALO_WIDTH) * HALO_GAIN * flow;
    color *= state.level * along;

    // Contact: hotspot and plume.
    let to_contact = vec2<f32>(s - beam.length, u);
    let r = length(to_contact);
    let flicker = 1.0 + HOT_FLICKER * (noise_2d(vec2<f32>(time * 25.0, beam.seed)) - 0.5) * 2.0;
    let hot_r = r / HOT_RADIUS;
    color += mix(vec3<f32>(1.0), LIGHT_COLOR, 0.25) * exp(-hot_r * hot_r) * HOT_GAIN * flicker * state.level;
    let plume_noise = noise_2d(vec2<f32>(atan2(to_contact.y, to_contact.x) * 3.0 + beam.seed, r / 6.0 - time * 8.0));
    let plume_r = r / PLUME_RADIUS;
    let plume_color = mix(vec3<f32>(1.0, 0.95, 0.85), LIGHT_COLOR, clamp(plume_r, 0.0, 1.0));
    color += plume_color * exp(-plume_r * plume_r) * (0.3 + plume_noise) * PLUME_GAIN * state.level;

    // Sparks thrown back off the target, within a cone around the reflected beam. Only sparks born
    // since firing started draw, and they vanish on release.
    if (r < SPARK_SPEED * SPARK_LIFE * 1.5 + 4.0) {
        for (var i = 0; i < SPARK_COUNT; i++) {
            let offset = random(u32(i), 23u);
            let beat = time / SPARK_LIFE + offset;
            let life = fract(beat);
            let born = (floor(beat) - offset) * SPARK_LIFE;
            let id = u32(i) * 7919u + u32(floor(beat)) * 104729u;
            let angle = PI + (random(id, 29u) - 0.5) * 2.0 * SPARK_SPREAD;
            let speed = SPARK_SPEED * (0.5 + random(id, 31u));
            let direction = vec2<f32>(cos(angle), sin(angle));
            let travel = life * SPARK_LIFE * speed * (1.0 - 0.4 * life);
            let head = direction * travel;
            let tail = direction * max(travel - SPARK_STREAK * speed, 0.0);
            let segment = head - tail;
            let t = clamp(dot(to_contact - tail, segment) / max(dot(segment, segment), 1e-4), 0.0, 1.0);
            let d = length(to_contact - tail - segment * t);
            let alive = select(0.0, 1.0, born >= state.start) * state.on;
            color += mix(vec3<f32>(1.0), LIGHT_COLOR, life) * exp(-d * d / 0.5) * (1.0 - life) * SPARK_GAIN * alive;
        }
    }

    // Barrel tip heat.
    let barrel = vec2<f32>(s + 3.0, u) / 4.0;
    let ember = mix(vec3<f32>(0.5, 0.04, 0.0), vec3<f32>(1.0, 0.55, 0.15), state.heat);
    color += ember * exp(-dot(barrel, barrel)) * state.heat * BARREL_GAIN;

    return vec4<f32>(color * edge_window(beam, s, u), 0.0);
}
