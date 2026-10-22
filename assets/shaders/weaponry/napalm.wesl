// Burning napalm and char marks for the ground cover canvas. Adjacent cells of the same kind
// join into a continuous surface. Game time controls expiry and fading; the shader clock
// animates the flames. Both shaders return premultiplied colour.

import bevy_sprite_render::mesh2d::view_bindings::globals;
import package::shaders::common::fire_ramp::{FIRE_HEAT_CAP, fire_ramp};
import package::shaders::common::game_time::game_time_span;
import package::shaders::common::value_noise::value_fbm_2d_octaves;
import package::shaders::grids::ground_cover::{GROUND_COVER_CHAR, GROUND_COVER_NAPALM, ground_cover_cell_entries, ground_cover_entries_at, ground_cover_shows_kind_at};
import package::shaders::weaponry::ground_fire::{
    GROUND_FIRE_OPEN_BOTTOM, GROUND_FIRE_OPEN_BOTTOM_LEFT, GROUND_FIRE_OPEN_BOTTOM_RIGHT, GROUND_FIRE_OPEN_LEFT,
    GROUND_FIRE_OPEN_RIGHT, GROUND_FIRE_OPEN_TOP, GROUND_FIRE_OPEN_TOP_LEFT, GROUND_FIRE_OPEN_TOP_RIGHT,
    ground_fire, ground_fire_past_boundary,
};

// ── Napalm ───────────────────────────────────────────────────────────────────
// Seconds from landing to full flames.
const NAPALM_IGNITION: f32 = 0.12;
// Extra intensity of the ignition flare, fading out by `NAPALM_FLARE_END` seconds.
const NAPALM_FLARE_BOOST: f32 = 0.6;
const NAPALM_FLARE_END: f32 = 0.6;
// Seconds over which the flames die down before the napalm ends.
const NAPALM_DIE_DOWN: f32 = 1.0;
// Extra heat per unit of intensity above a single patch's full burn.
const NAPALM_STACK_HEAT: f32 = 0.12;
// Maximum extra heat from overlapping patches.
const NAPALM_STACK_HEAT_CAP: f32 = 0.3;

// ── Char ─────────────────────────────────────────────────────────────────────
// Ground darkening of one fresh char mark.
const CHAR_DARKNESS: f32 = 0.5;
// Exponential cinder decay rate over the mark's normalized age.
const CHAR_CINDER_DECAY: f32 = 6.0;

// Opens boundaries toward visible neighbouring cells of the same kind.
fn open_sides(cell: vec2<i32>, kind: u32) -> u32 {
    let right = ground_cover_shows_kind_at(cell + vec2<i32>(1, 0), kind);
    let top = ground_cover_shows_kind_at(cell + vec2<i32>(0, 1), kind);
    let left = ground_cover_shows_kind_at(cell + vec2<i32>(-1, 0), kind);
    let bottom = ground_cover_shows_kind_at(cell + vec2<i32>(0, -1), kind);
    var open = 0u;
    if right { open |= GROUND_FIRE_OPEN_RIGHT; }
    if top { open |= GROUND_FIRE_OPEN_TOP; }
    if left { open |= GROUND_FIRE_OPEN_LEFT; }
    if bottom { open |= GROUND_FIRE_OPEN_BOTTOM; }
    if right && top && ground_cover_shows_kind_at(cell + vec2<i32>(1, 1), kind) { open |= GROUND_FIRE_OPEN_TOP_RIGHT; }
    if left && top && ground_cover_shows_kind_at(cell + vec2<i32>(-1, 1), kind) { open |= GROUND_FIRE_OPEN_TOP_LEFT; }
    if left && bottom && ground_cover_shows_kind_at(cell + vec2<i32>(-1, -1), kind) { open |= GROUND_FIRE_OPEN_BOTTOM_LEFT; }
    if right && bottom && ground_cover_shows_kind_at(cell + vec2<i32>(1, -1), kind) { open |= GROUND_FIRE_OPEN_BOTTOM_RIGHT; }
    return open;
}

// A patch flares on ignition, burns steadily, then fades before expiry.
fn napalm_intensity(starts_at: f32, ends_at: f32) -> f32 {
    let span = game_time_span(starts_at, ends_at);
    let ignition = smoothstep(0.0, NAPALM_IGNITION, span.age);
    let flare = 1.0 + NAPALM_FLARE_BOOST * (1.0 - smoothstep(NAPALM_IGNITION, NAPALM_FLARE_END, span.age));
    let dying = saturate(span.remaining / NAPALM_DIE_DOWN);
    return ignition * flare * dying;
}

// `local` is the point relative to the centre of `cell`, `world` the same point in world space.
fn napalm_look(local: vec2<f32>, world: vec2<f32>, cell: vec2<i32>) -> vec4<f32> {
    var intensity = 0.0;
    let range = ground_cover_entries_at(cell);
    for (var index = range.x; index < range.y; index++) {
        let entry = ground_cover_cell_entries[index];
        if entry.kind == GROUND_COVER_NAPALM {
            intensity += napalm_intensity(entry.starts_at, entry.ends_at);
        }
    }
    if intensity <= 0.0 {
        return vec4<f32>(0.0);
    }

    let fire = ground_fire(local, world, globals.time, open_sides(cell, GROUND_COVER_NAPALM));
    let strength = min(intensity, 1.0);
    let stack_heat = min(NAPALM_STACK_HEAT * max(intensity - 1.0, 0.0), NAPALM_STACK_HEAT_CAP);
    let heat = min(0.45 + 0.15 * fire.coverage + stack_heat, FIRE_HEAT_CAP);
    let fire_alpha = saturate(fire.coverage * strength);
    let rim = fire.rim * strength;
    let color = fire_ramp(heat) * fire_alpha * 1.2 + fire_ramp(0.5) * rim;
    return vec4<f32>(color, fire_alpha * 0.85 + rim * 0.3);
}

// Char marks fade over their lifetimes, with cinders cooling first.
// Overlapping marks darken the ground further.
fn char_look(local: vec2<f32>, world: vec2<f32>, cell: vec2<i32>) -> vec4<f32> {
    var transmittance = 1.0;
    var cinder_heat = 0.0;
    let range = ground_cover_entries_at(cell);
    for (var index = range.x; index < range.y; index++) {
        let entry = ground_cover_cell_entries[index];
        if entry.kind == GROUND_COVER_CHAR {
            let span = game_time_span(entry.starts_at, entry.ends_at);
            let life = saturate(span.age / max(span.duration, 0.001));
            transmittance *= 1.0 - CHAR_DARKNESS * (1.0 - smoothstep(0.3, 1.0, life));
            cinder_heat += exp(-life * CHAR_CINDER_DECAY);
        }
    }
    if transmittance >= 1.0 {
        return vec4<f32>(0.0);
    }

    let past = ground_fire_past_boundary(local, open_sides(cell, GROUND_COVER_CHAR));
    let noise = value_fbm_2d_octaves(world / 4.0, 4);
    let mark = (1.0 - smoothstep(-4.0, 2.0, past + (noise - 0.5) * 8.0)) * smoothstep(0.35, 0.6, noise + 0.15);
    let cinder_noise = value_fbm_2d_octaves(world / 2.5 + vec2<f32>(53.0, 17.0), 3);
    let cinders = smoothstep(0.66, 0.72, cinder_noise) * mark * min(cinder_heat, 1.0);
    return vec4<f32>(fire_ramp(0.55) * cinders, mark * (1.0 - transmittance));
}
