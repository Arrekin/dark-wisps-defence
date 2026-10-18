//! # Flame Stream Fire
//!
//! Visual entities for `FlameStream`, each a lane visual on a layer of its own:
//!
//! - `Flame` — stream jet from `nozzle` to the hit point (or `head`), with a `FlameGlow` child for
//!   additive ground light.
//! - `ImpactFireVisual` — splash, clinging flames, cell fire, and sparks on the hit tile, driven by
//!   the burn state in `ImpactFire`.
//! - `FlameScorch` — transient ground decal spawned when a tile stops burning.
//! - `PilotFlame` — small nozzle flame visible while the stream is armed.
//!
//! `Flame`, the impact fire, and `PilotFlame` are spawned as children of `FlameStream`.

use bevy::{prelude::*, reflect::TypePath, render::render_resource::{BlendState, ShaderType}};

use game_core::prelude::{GridCoords, GridImprint, MapBound, ZDepth};
use session::GameClock;
use states::prelude::GameState;
use visuals::{
    lane::{LanePlugin, LaneVisual},
    shader::{ADDITIVE_BLENDING, seed_from_entity},
    transient_visual::{TransientVisual, despawn_played_out},
};
use weaponry::{flame_stream::FlameStreamSystems, prelude::*};

pub(crate) struct FlameStreamFirePlugin;
impl Plugin for FlameStreamFirePlugin {
    fn build(&self, app: &mut App) {
        app
            .add_plugins((
                LanePlugin::<Flame>::default(),
                LanePlugin::<FlameGlow>::default(),
                LanePlugin::<ImpactFireVisual>::default(),
                LanePlugin::<FlameScorch>::default(),
                LanePlugin::<PilotFlame>::default(),
            ))
            .add_systems(Update, (
                (update_flames, update_pilot_flames).after(FlameStreamSystems),
                (
                    update_impact_fires.after(FlameStreamSystems),
                    despawn_played_out::<FlameScorch>,
                ).run_if(in_state(GameState::Running)),
            ))
            .add_observer(on_add_flame_stream_spawn_fire);
    }
}

// Stream mesh
/// Multiplier on max cone half-width to fit edge noise and soot.
const FLAME_MESH_MARGIN: f32 = 1.6;
/// Extra mesh half-width in world units to fit head/pileup bulges.
const FLAME_MESH_PADDING: f32 = 13.;
/// Extra mesh length beyond `stream.length` for tip burnout and soot.
const FLAME_MESH_OVERHANG: f32 = 30.;

// Ground light
const GLOW_MESH_HALF_WIDTH: f32 = 100.;
/// Extra glow mesh length beyond each end of the stream mesh.
const GLOW_MESH_OVERHANG: f32 = 80.;
/// Glow intensity multiplier while venting.
const GLOW_VENTING: f32 = 0.6;

// Impact
/// Intensity ramp-up rate (per second) on an active hit tile.
const IMPACT_IGNITE_RATE: f32 = 12.;
/// Intensity decay rate (per second) after a tile is no longer hit.
const IMPACT_FADE_RATE: f32 = 4.;
const IMPACT_MESH_HALF_SIZE: f32 = 70.;

// Scorch
/// Burn duration (seconds) required for a full-strength scorch mark.
const SCORCH_BUILDUP: f32 = 1.;
const SCORCH_DURATION: f32 = 1.5;
const SCORCH_MESH_HALF_SIZE: f32 = 24.;

// Pilot flame
const PILOT_MESH_SIZE: f32 = 14.;
/// Axial offset of the pilot flame root ahead of `nozzle`.
const PILOT_OFFSET: f32 = 1.5;

/// Stream jet quad. Child of `FlameStream`. Field order and types mirror `Instance` in
/// `assets/shaders/weaponry/flame_stream.wesl`.
#[derive(Component, ShaderType, TypePath, Clone, Default, PartialEq)]
#[require(ZDepth::FlameJet)]
struct Flame {
    seed: f32,
    length: f32,
    half_width: f32,
    nozzle_half_width: f32,
    spread: f32,
    head: f32,
    tail: f32,
    end: f32,
    hit: f32,
    venting: f32,
}
impl LaneVisual for Flame {
    const LABEL: &'static str = "flame_jet";
    const LOOK: &'static str = "shaders::weaponry::flame_stream";

    /// Premultiplied alpha: additive flame emission with alpha-blended soot.
    fn blend() -> BlendState {
        BlendState::PREMULTIPLIED_ALPHA_BLENDING
    }
}

/// Additive ground light along the stream and around the hit. Child of `Flame`. Field order and
/// types mirror `Instance` in `assets/shaders/weaponry/flame_stream_glow.wesl`.
#[derive(Component, ShaderType, TypePath, Clone, Default, PartialEq)]
#[require(ZDepth::FlameGlow)]
struct FlameGlow {
    intensity: f32,
    near: f32,
    far: f32,
    half_width: f32,
    tail: f32,
    end: f32,
    hit: f32,
}
impl LaneVisual for FlameGlow {
    const LABEL: &'static str = "flame_glow";
    const LOOK: &'static str = "shaders::weaponry::flame_stream_glow";

    fn blend() -> BlendState {
        ADDITIVE_BLENDING
    }
}

/// Burn state of the hit tile, stored with `ImpactFireVisual` on a child of `FlameStream`.
#[derive(Component, Default)]
struct ImpactFire {
    /// Normalized intensity in `0.0..=1.0`.
    intensity: f32,
    /// Last burned tile, retained while fading out.
    coords: Option<GridCoords>,
    /// Cumulative burn time (seconds) on `coords`.
    burning_for: f32,
}
impl ImpactFire {
    /// Advances burn state on `coords` by `delta` seconds. If `coords` changed, resets intensity
    /// and returns the previous tile's `BurntTile` data.
    fn burn(&mut self, coords: GridCoords, delta: f32) -> Option<BurntTile> {
        let mut burnt = None;
        if self.coords != Some(coords) {
            burnt = self.stop_burning();
            self.coords = Some(coords);
            self.intensity = 0.;
        }
        self.burning_for += delta;
        self.intensity = (self.intensity + IMPACT_IGNITE_RATE * delta).min(1.);
        burnt
    }

    /// Fades intensity by `delta` seconds and flushes any active `BurntTile`.
    fn fade(&mut self, delta: f32) -> Option<BurntTile> {
        self.intensity = (self.intensity - IMPACT_FADE_RATE * delta).max(0.);
        self.stop_burning()
    }

    fn stop_burning(&mut self) -> Option<BurntTile> {
        if self.burning_for <= 0. { return None; }
        let coords = self.coords?;
        let strength = (self.burning_for / SCORCH_BUILDUP).min(1.);
        self.burning_for = 0.;
        Some(BurntTile { coords, strength })
    }
}

struct BurntTile {
    coords: GridCoords,
    /// Normalized scorch strength in `0.0..=1.0`.
    strength: f32,
}

/// Fire on the currently hit tile. Field order and types mirror `Instance` in
/// `assets/shaders/weaponry/flame_stream_impact.wesl`.
#[derive(Component, ShaderType, TypePath, Clone, Default, PartialEq)]
#[require(ZDepth::FlameImpact)]
struct ImpactFireVisual {
    intensity: f32,
    seed: f32,
    half_size: f32,
    hit_point: Vec2,
    direction: Vec2,
}
impl LaneVisual for ImpactFireVisual {
    const LABEL: &'static str = "flame_impact";
    const LOOK: &'static str = "shaders::weaponry::flame_stream_impact";

    fn quad_size() -> Vec2 {
        Vec2::splat(2. * IMPACT_MESH_HALF_SIZE)
    }

    fn blend() -> BlendState {
        BlendState::PREMULTIPLIED_ALPHA_BLENDING
    }
}

/// Temporary scorch decal spawned when a tile stops burning.
/// Field order and types mirror `Instance` in `assets/shaders/weaponry/flame_stream_scorch.wesl`.
#[derive(Component, ShaderType, TypePath, Clone)]
#[require(MapBound, ZDepth::FlameScorch)]
struct FlameScorch {
    /// Wrapped game time the tile stopped burning.
    start_time: f32,
    duration: f32,
    /// Normalized char intensity in `0.0..=1.0`, from the burn duration.
    strength: f32,
    seed: f32,
    half_size: f32,
}
impl LaneVisual for FlameScorch {
    const LABEL: &'static str = "flame_scorch";
    const LOOK: &'static str = "shaders::weaponry::flame_stream_scorch";

    fn quad_size() -> Vec2 {
        Vec2::splat(2. * SCORCH_MESH_HALF_SIZE)
    }

    /// Premultiplied alpha: darkens ground via alpha while cinders add light.
    fn blend() -> BlendState {
        BlendState::PREMULTIPLIED_ALPHA_BLENDING
    }
}
impl TransientVisual for FlameScorch {
    fn start_time(&self) -> f32 { self.start_time }
    fn duration(&self) -> f32 { self.duration }
}

/// Nozzle pilot flame shown while the stream is armed. Child of `FlameStream`. Field order and
/// types mirror `Instance` in `assets/shaders/weaponry/flame_stream_pilot.wesl`.
#[derive(Component, ShaderType, TypePath, Clone)]
#[require(ZDepth::FlamePilot)]
struct PilotFlame {
    seed: f32,
    size: f32,
}
impl LaneVisual for PilotFlame {
    const LABEL: &'static str = "flame_pilot";
    const LOOK: &'static str = "shaders::weaponry::flame_stream_pilot";

    fn quad_size() -> Vec2 {
        Vec2::splat(PILOT_MESH_SIZE)
    }

    fn blend() -> BlendState {
        ADDITIVE_BLENDING
    }
}

fn on_add_flame_stream_spawn_fire(
    trigger: On<Add<FlameStream>>,
    mut commands: Commands,
) {
    let seed = seed_from_entity(trigger.entity);
    commands.entity(trigger.entity).insert(children![
        (
            Flame { seed, ..default() },
            Visibility::Hidden,
            children![FlameGlow::default()],
        ),
        (
            ImpactFire::default(),
            ImpactFireVisual { seed, half_size: IMPACT_MESH_HALF_SIZE, ..default() },
            Visibility::Hidden,
        ),
        (
            PilotFlame { seed, size: PILOT_MESH_SIZE },
            Visibility::Hidden,
        ),
    ]);
}

fn update_flames(
    mut flames: Query<(&ChildOf, &Children, &mut Transform, &mut Visibility, &mut Flame)>,
    mut glows: Query<(&mut Transform, &mut FlameGlow), Without<Flame>>,
    streams: Query<&FlameStream>,
) {
    for (child_of, children, mut transform, mut visibility, mut flame) in flames.iter_mut() {
        let Ok(stream) = streams.get(child_of.parent()) else { continue; };
        let Some(fuel) = stream.fuel() else {
            visibility.set_if_neq(Visibility::Hidden);
            continue;
        };
        visibility.set_if_neq(Visibility::Inherited);

        let nozzle = stream.nozzle;
        let direction = stream.direction;
        let length = stream.length + FLAME_MESH_OVERHANG;
        let half_width = stream.half_width_at(length) * FLAME_MESH_MARGIN + FLAME_MESH_PADDING;
        transform.set_if_neq(Transform {
            translation: (nozzle + direction * length * 0.5).extend(transform.translation.z),
            rotation: Quat::from_rotation_z(direction.to_angle()),
            scale: Vec3::new(length, 2. * half_width, 1.),
        });

        let (tail, head) = fuel.into_inner();
        let end = stream.hit.as_ref().map_or(head, |hit| hit.distance);
        let hit = f32::from(stream.hit.is_some());
        let venting = stream.is_venting();
        flame.set_if_neq(Flame {
            length,
            half_width,
            nozzle_half_width: stream.half_width_at(0.),
            spread: stream.spread(),
            head,
            tail,
            end,
            hit,
            venting: f32::from(venting),
            ..*flame
        });

        // Child of `Flame`: scale is relative to the parent stream quad.
        for &child in children {
            let Ok((mut glow_transform, mut glow)) = glows.get_mut(child) else { continue; };
            glow_transform.set_if_neq(Transform {
                scale: Vec3::new((length + 2. * GLOW_MESH_OVERHANG) / length, GLOW_MESH_HALF_WIDTH / half_width, 1.),
                ..*glow_transform
            });
            glow.set_if_neq(FlameGlow {
                intensity: if venting { GLOW_VENTING } else { 1. },
                near: -GLOW_MESH_OVERHANG,
                far: length + GLOW_MESH_OVERHANG,
                half_width: GLOW_MESH_HALF_WIDTH,
                tail,
                end,
                hit,
            });
        }
    }
}

fn update_impact_fires(
    mut commands: Commands,
    time: Res<Time>,
    clock: Res<GameClock>,
    mut impact_fires: Query<(&ChildOf, &mut ImpactFire, &mut ImpactFireVisual, &mut Transform, &mut Visibility)>,
    streams: Query<&FlameStream>,
) {
    let delta = time.delta_secs();
    for (child_of, mut impact_fire, mut visual, mut transform, mut visibility) in impact_fires.iter_mut() {
        let Ok(stream) = streams.get(child_of.parent()) else { continue; };
        let mut next = visual.clone();

        let burnt = match &stream.hit {
            Some(hit) => {
                let tile_center = hit.coords.to_world_position_centered(GridImprint::default());
                transform.set_if_neq(Transform { translation: tile_center.extend(transform.translation.z), ..*transform });
                next.hit_point = stream.nozzle + stream.direction * hit.distance - tile_center;
                next.direction = stream.direction;
                impact_fire.burn(hit.coords, delta)
            }
            None => impact_fire.fade(delta),
        };
        next.intensity = impact_fire.intensity;
        visual.set_if_neq(next);
        visibility.set_if_neq(if impact_fire.intensity > 0. { Visibility::Inherited } else { Visibility::Hidden });

        if let Some(burnt) = burnt {
            commands.spawn((
                FlameScorch {
                    start_time: clock.wrapped(),
                    duration: SCORCH_DURATION,
                    strength: burnt.strength,
                    seed: (burnt.coords.x * 31 + burnt.coords.y) as f32 % 97.,
                    half_size: SCORCH_MESH_HALF_SIZE,
                },
                Transform::from_translation(burnt.coords.to_world_position_centered(GridImprint::default()).extend(0.)),
            ));
        }
    }
}

fn update_pilot_flames(
    mut pilots: Query<(&ChildOf, &mut Transform, &mut Visibility), With<PilotFlame>>,
    streams: Query<&FlameStream>,
) {
    for (child_of, mut transform, mut visibility) in pilots.iter_mut() {
        let Ok(stream) = streams.get(child_of.parent()) else { continue; };
        let armed = stream.is_armed();
        visibility.set_if_neq(if armed { Visibility::Inherited } else { Visibility::Hidden });
        if !armed { continue; }

        let direction = stream.direction;
        let root = stream.nozzle + direction * PILOT_OFFSET;
        transform.set_if_neq(Transform {
            translation: (root + direction * PILOT_MESH_SIZE * 0.5).extend(transform.translation.z),
            rotation: Quat::from_rotation_z(direction.to_angle()),
            ..*transform
        });
    }
}
