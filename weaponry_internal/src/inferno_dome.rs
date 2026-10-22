//! # Inferno Dome
//!
//! Temporary fire dome spawned when a `TowerInferno` discharges.
//! Its child visuals share a start time but have separate durations:
//!
//! - `InfernoDomeFire` — hemisphere shell, interior billows, rim spill light, and falling embers.
//! - `InfernoDomeGround` — release flash and ground scorch decal.
//! - `InfernoDomeDisplacement` — lens bulge and heat shimmer applied before the air band.
//!   It distorts the ground and surface bands while leaving the dome's fire undistorted.
//!
//! Each part derives its age from the game clock without per-frame instance updates.
//! The dome despawns when its longest effect, the distortion, expires. These visuals are not saved.

use bevy::{prelude::*, reflect::TypePath, render::render_resource::{BlendState, ShaderType}};

use game_core::prelude::ZDepth;
use session::{GameClock, TimeBound};
use visuals::{
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
                LanePlugin::<InfernoDomeGround>::default(),
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

// Ground
/// Active duration (seconds) of `InfernoDomeGround`. Matches `SCORCH_DURATION` in
/// `assets/shaders/weaponry/inferno_dome_ground.wesl`.
const GROUND_DURATION: f32 = 2.5;
/// Extra mesh half-size beyond `radius` for ragged scorch edges.
const GROUND_MESH_MARGIN: f32 = 24.;

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
#[require(ZDepth::AirEffect)]
struct InfernoDomeFire {
    /// Wrapped game time of the discharge.
    start_time: f32,
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

/// Ground flash and scorch quad. Child of `InfernoDome`. Field order and types mirror `Instance` in
/// `assets/shaders/weaponry/inferno_dome_ground.wesl`.
#[derive(Component, ShaderType, TypePath, Clone)]
#[require(ZDepth::GroundDecal)]
struct InfernoDomeGround {
    /// Wrapped game time of the discharge.
    start_time: f32,
    radius: f32,
    seed: f32,
    /// Quad half-size in world units.
    half_size: f32,
}
impl LaneVisual for InfernoDomeGround {
    const LABEL: &'static str = "inferno_dome_ground";
    const LOOK: &'static str = "shaders::weaponry::inferno_dome_ground";

    /// Premultiplied alpha: darkens ground via scorch alpha while release flash adds light.
    fn blend() -> BlendState {
        BlendState::PREMULTIPLIED_ALPHA_BLENDING
    }
}

/// Lens bulge and heat shimmer applied before the air band. Spawned as a child of `InfernoDome`.
/// Field order and types mirror `Instance` in
/// `assets/shaders/weaponry/inferno_dome_displacement.wesl`.
#[derive(Component, ShaderType, TypePath, Clone)]
#[require(ZDepth::AirEffect)]
struct InfernoDomeDisplacement {
    /// Wrapped game time of the discharge.
    start_time: f32,
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
    let start_time = clock.wrapped();
    let seed = seed_from_entity(entity);
    let fire_half_size = dome.radius + FIRE_MESH_MARGIN;
    let ground_half_size = dome.radius + GROUND_MESH_MARGIN;
    // The dome lasts as long as its longest part. The displacement child despawns with it.
    commands.entity(entity).insert((
        TimeBound::lasting(&clock, FIRE_DURATION.max(GROUND_DURATION).max(SHIMMER_DURATION)),
        children![
            (
                InfernoDomeFire { start_time, radius: dome.radius, seed, half_size: fire_half_size },
                TimeBound::lasting(&clock, FIRE_DURATION),
                Transform::from_scale(Vec3::new(2. * fire_half_size, 2. * fire_half_size, 1.)),
            ),
            (
                InfernoDomeGround { start_time, radius: dome.radius, seed, half_size: ground_half_size },
                TimeBound::lasting(&clock, GROUND_DURATION),
                Transform::from_scale(Vec3::new(2. * ground_half_size, 2. * ground_half_size, 1.)),
            ),
            InfernoDomeDisplacement { start_time, radius: dome.radius, half_size: dome.radius + SHIMMER_MARGIN },
        ],
    ));
}
