// Burning napalm and char marks for the floor cover canvas. Neighbouring cells of the same kind
// whose floors connect join into a continuous surface. Game time controls expiry and fading; the
// shader clock animates the flames. Both shaders return premultiplied colour.
//
// Napalm and char entities always carry a `TimeBound`, so `entry.duration` is always a finite
// positive lifetime (`FLOOR_COVER_PERMANENT_DURATION` is never used here).

import bevy_sprite_render::mesh2d::view_bindings::globals;
import package::shaders::common::clock::game_time_since;
import package::shaders::common::fire_ramp::{FIRE_HEAT_CAP, fire_ramp};
import package::shaders::common::noise::noise_fbm_2d;
import package::shaders::grids::floor_connections::connects_floor;
import package::shaders::grids::floor_cover::{
    FLOOR_COVER_CHAR, FLOOR_COVER_NAPALM, FloorCoverEntry, floor_cover_cell_entries, floor_cover_entries_at, floor_cover_has_kind_at,
};
import package::shaders::grids::obstacles::LEVEL_NONE;
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
// Floor darkening of one fresh char mark.
const CHAR_DARKNESS: f32 = 0.5;
// Exponential cinder decay rate over the mark's normalized age.
const CHAR_CINDER_DECAY: f32 = 6.0;

// Level of the floor across `side` that connects with the floor of `level` at `cell` and holds a
// cover of `kind`, or `LEVEL_NONE`.
fn joined_level(cell: vec2<i32>, level: u32, side: vec2<i32>, kind: u32) -> u32 {
    let across = connects_floor(cell, level, side);
    if across == LEVEL_NONE || !floor_cover_has_kind_at(cell + side, kind, across) { return LEVEL_NONE; }
    return across;
}

// True if the diagonal neighbour in direction `first + second` holds a cover of `kind` on the floor
// that both side neighbours' floors connect to.
fn joins_corner(cell: vec2<i32>, first: vec2<i32>, first_joined: u32, second: vec2<i32>, second_joined: u32, kind: u32) -> bool {
    if first_joined == LEVEL_NONE || second_joined == LEVEL_NONE { return false; }
    let through_first = joined_level(cell + first, first_joined, second, kind);
    return through_first != LEVEL_NONE && through_first == joined_level(cell + second, second_joined, first, kind);
}

// Opens boundaries toward neighbouring cells whose floors connect with this cell's floor and hold
// a cover of the same kind.
fn open_sides(cell: vec2<i32>, kind: u32, level: u32) -> u32 {
    let right = vec2<i32>(1, 0);
    let top = vec2<i32>(0, 1);
    let left = vec2<i32>(-1, 0);
    let bottom = vec2<i32>(0, -1);
    let right_joined = joined_level(cell, level, right, kind);
    let top_joined = joined_level(cell, level, top, kind);
    let left_joined = joined_level(cell, level, left, kind);
    let bottom_joined = joined_level(cell, level, bottom, kind);
    var open = 0u;
    if right_joined != LEVEL_NONE { open |= GROUND_FIRE_OPEN_RIGHT; }
    if top_joined != LEVEL_NONE { open |= GROUND_FIRE_OPEN_TOP; }
    if left_joined != LEVEL_NONE { open |= GROUND_FIRE_OPEN_LEFT; }
    if bottom_joined != LEVEL_NONE { open |= GROUND_FIRE_OPEN_BOTTOM; }
    if joins_corner(cell, right, right_joined, top, top_joined, kind) { open |= GROUND_FIRE_OPEN_TOP_RIGHT; }
    if joins_corner(cell, left, left_joined, top, top_joined, kind) { open |= GROUND_FIRE_OPEN_TOP_LEFT; }
    if joins_corner(cell, left, left_joined, bottom, bottom_joined, kind) { open |= GROUND_FIRE_OPEN_BOTTOM_LEFT; }
    if joins_corner(cell, right, right_joined, bottom, bottom_joined, kind) { open |= GROUND_FIRE_OPEN_BOTTOM_RIGHT; }
    return open;
}

// A patch flares on ignition, burns steadily, then fades before expiry.
fn napalm_intensity(entry: FloorCoverEntry) -> f32 {
    let age = game_time_since(entry.start);
    let ignition = smoothstep(0.0, NAPALM_IGNITION, age);
    let flare = 1.0 + NAPALM_FLARE_BOOST * (1.0 - smoothstep(NAPALM_IGNITION, NAPALM_FLARE_END, age));
    let dying = saturate((entry.duration - age) / NAPALM_DIE_DOWN);
    return ignition * flare * dying;
}

// `local` is the point relative to the centre of `cell`, `world` the same point in world space.
// `level` is the level code (LEVEL_* in grids/obstacles.wesl) of the drawn covers.
fn napalm_look(local: vec2<f32>, world: vec2<f32>, cell: vec2<i32>, level: u32) -> vec4<f32> {
    var intensity = 0.0;
    let range = floor_cover_entries_at(cell);
    for (var index = range.x; index < range.y; index++) {
        let entry = floor_cover_cell_entries[index];
        if entry.kind == FLOOR_COVER_NAPALM && entry.level == level {
            intensity += napalm_intensity(entry);
        }
    }
    if intensity <= 0.0 {
        return vec4<f32>(0.0);
    }

    let fire = ground_fire(local, world, globals.time, open_sides(cell, FLOOR_COVER_NAPALM, level));
    let strength = min(intensity, 1.0);
    let stack_heat = min(NAPALM_STACK_HEAT * max(intensity - 1.0, 0.0), NAPALM_STACK_HEAT_CAP);
    let heat = min(0.45 + 0.15 * fire.coverage + stack_heat, FIRE_HEAT_CAP);
    let fire_alpha = saturate(fire.coverage * strength);
    let rim = fire.rim * strength;
    let color = fire_ramp(heat) * fire_alpha * 1.2 + fire_ramp(0.5) * rim;
    return vec4<f32>(color, fire_alpha * 0.85 + rim * 0.3);
}

// Char marks fade over their lifetimes, with cinders cooling first.
// Overlapping marks darken the floor further.
fn char_look(local: vec2<f32>, world: vec2<f32>, cell: vec2<i32>, level: u32) -> vec4<f32> {
    var transmittance = 1.0;
    var cinder_heat = 0.0;
    let range = floor_cover_entries_at(cell);
    for (var index = range.x; index < range.y; index++) {
        let entry = floor_cover_cell_entries[index];
        if entry.kind == FLOOR_COVER_CHAR && entry.level == level {
            let life = saturate(game_time_since(entry.start) / entry.duration);
            transmittance *= 1.0 - CHAR_DARKNESS * (1.0 - smoothstep(0.3, 1.0, life));
            cinder_heat += exp(-life * CHAR_CINDER_DECAY);
        }
    }
    if transmittance >= 1.0 {
        return vec4<f32>(0.0);
    }

    let past = ground_fire_past_boundary(local, open_sides(cell, FLOOR_COVER_CHAR, level));
    let noise = noise_fbm_2d(world / 4.0, 4);
    let mark = (1.0 - smoothstep(-4.0, 2.0, past + (noise - 0.5) * 8.0)) * smoothstep(0.35, 0.6, noise + 0.15);
    let cinder_noise = noise_fbm_2d(world / 2.5 + vec2<f32>(53.0, 17.0), 3);
    let cinders = smoothstep(0.66, 0.72, cinder_noise) * mark * min(cinder_heat, 1.0);
    return vec4<f32>(fire_ramp(0.55) * cinders, mark * (1.0 - transmittance));
}
