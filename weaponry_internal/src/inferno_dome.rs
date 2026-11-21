//! # Inferno Dome
//!
//! Temporary fire dome spawned when a `TowerInferno` discharges.
//! Its child visuals share a start instant but have separate durations:
//!
//! - `InfernoDomeFire` — hemisphere shell, interior billows, rim spill light, and falling embers.
//!   Its outer rim adjusts for terrain height, stepping inward on walls and buildings and expanding
//!   over pits (`assets/shaders/visuals/terrain_rim.wesl`).
//! - `InfernoDomeDisplacement` — lens bulge and heat shimmer applied before the air band.
//!   It distorts the ground and surface bands while leaving the dome's fire undistorted.
//!
//! Each part derives its age from the game clock without per-frame instance updates.
//! The dome despawns when its longest effect, the distortion, expires. These visuals are not saved.

use bevy::{prelude::*, reflect::TypePath, render::render_resource::{BlendState, ShaderType}};

use game_core::prelude::{DrawLevel, ZDepth};
use session::{GameClock, TimeBound};
use visuals::{
    clock::ClockInstant,
    lane::{LaneParts, LanePlugin, LaneVisual},
    shader::seed_from_entity,
};
use weaponry::prelude::*;

pub(crate) struct InfernoDomePlugin;
impl Plugin for InfernoDomePlugin {
    fn build(&self, app: &mut App) {
        app
            .add_plugins((
                LanePlugin::<InfernoDomeFire>::default(),
                LanePlugin::<InfernoDomeDisplacement>::default(),
            ))
            .add_observer(on_add_inferno_dome_spawn_parts);
    }
}

// Fire
/// Active duration (seconds) of `InfernoDomeFire`. Matches breakup + ember lifetimes in
/// `assets/shaders/weaponry/inferno_dome.wesl`.
const FIRE_DURATION: f32 = 1.6;
/// Extra mesh half-size beyond `radius` for rim noise and spilled light.
const FIRE_MESH_MARGIN: f32 = 50.;

// Shimmer
/// Active duration (seconds) of the dome distortion. Matches `SHIMMER_DURATION` in
/// `assets/shaders/weaponry/inferno_dome_displacement.wesl`.
const SHIMMER_DURATION: f32 = 3.5;
/// Margin beyond `radius` that the shimmer fades across.
const SHIMMER_MARGIN: f32 = 8.;

/// Dome fire drawn in the air band, after its distortion. Spawned as a child of `InfernoDome`.
/// Field order and types mirror `Instance` in
/// `assets/shaders/weaponry/inferno_dome.wesl`.
#[derive(Component, ShaderType, TypePath, Clone)]
#[require(ZDepth::AirEffect, DrawLevel::Airborne)]
struct InfernoDomeFire {
    /// Game clock instant of the discharge.
    start: ClockInstant,
    radius: f32,
    seed: f32,
    /// Quad half-size in world units.
    half_size: f32,
}
impl LaneVisual for InfernoDomeFire {
    const LABEL: &'static str = "inferno_dome_fire";
    const LOOK: &'static str = "shaders::weaponry::inferno_dome";

    /// Premultiplied alpha: opaque/semi-opaque flame shell with additive spill light and embers.
    fn blend() -> BlendState {
        BlendState::PREMULTIPLIED_ALPHA_BLENDING
    }
}

/// Lens bulge and heat shimmer applied before the air band. Spawned as a child of `InfernoDome`.
/// Field order and types mirror `Instance` in
/// `assets/shaders/weaponry/inferno_dome_displacement.wesl`.
#[derive(Component, ShaderType, TypePath, Clone)]
#[require(ZDepth::AirEffect, DrawLevel::Airborne)]
struct InfernoDomeDisplacement {
    /// Game clock instant of the discharge.
    start: ClockInstant,
    radius: f32,
    /// Quad half-size in world units.
    half_size: f32,
}
impl LaneVisual for InfernoDomeDisplacement {
    const LABEL: &'static str = "inferno_dome_displacement";
    const LOOK: &'static str = "shaders::weaponry::inferno_dome_displacement";
    const PARTS: LaneParts = LaneParts::DISPLACE;

    fn cut_quad_size(&self) -> Vec2 {
        Vec2::splat(2. * self.half_size)
    }
}

fn on_add_inferno_dome_spawn_parts(
    trigger: On<Add<InfernoDome>>,
    mut commands: Commands,
    clock: Res<GameClock>,
    domes: Query<&InfernoDome>,
) {
    let entity = trigger.entity;
    let Ok(dome) = domes.get(entity) else { return; };
    let start = ClockInstant::at(clock.now());
    let seed = seed_from_entity(entity);
    let fire_half_size = dome.radius + FIRE_MESH_MARGIN;
    // The dome lasts as long as its longest part. The displacement child despawns with it.
    commands.entity(entity).insert((
        TimeBound::lasting(&clock, FIRE_DURATION.max(SHIMMER_DURATION)),
        children![
            (
                InfernoDomeFire { start, radius: dome.radius, seed, half_size: fire_half_size },
                TimeBound::lasting(&clock, FIRE_DURATION),
                Transform::from_scale(Vec3::new(2. * fire_half_size, 2. * fire_half_size, 1.)),
            ),
            InfernoDomeDisplacement { start, radius: dome.radius, half_size: dome.radius + SHIMMER_MARGIN },
        ],
    ));
}
