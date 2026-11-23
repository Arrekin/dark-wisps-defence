//! # Fireball
//!
//! Blaze Mortar projectile. On landing, detonates a 2-step (5x5) blast via [`BlastHitManager`].
//! Wisps in hit cells within [`MAIN_BLAST_REACH`] (1 step) take full damage; those in the outer
//! [`RING_REACH`] (2 steps) take [`RING_DAMAGE_SHARE`] of full damage.
//!
//! ## Rendering
//!
//! - **In flight:** `FireballBody` faces the travel direction with a speed-scaled tail (shortest at
//!   the apex). `FireballGlow` projects an additive light pool on the ground that widens and dims
//!   with height.
//! - **Impact:** A single shader (`fireball_impact.wesl`) renders two `TimeBound` entities from the
//!   blast's [`ImpactReachMasks`]: the `Blast` layer (drawn at `OnElevation` above wisps) and the
//!   longer-lived `Scorch` layer (drawn at the blast's altitude level below wisps).
//!
//! Body, glow, and impact each use a [`LanePlugin`] for instanced batching. Impact instances store
//! a start timestamp and duration so the shader computes age directly from the game clock.

use bevy::{
    prelude::*,
    reflect::TypePath,
    render::render_resource::{BlendState, ShaderType},
};

use alteration::modifiers::prelude::AttackDamage;
use game_core::prelude::{CELL_SIZE, DamageMessage, DrawLevel, GridCoords, MapBound, OptionalCommands, Property, ZDepth};
use logging::prelude::*;
use persistence::{prelude::*, rusqlite};
use session::{GameClock, TimeBound};
use states::prelude::MapLoadingStage;
use visuals::{
    clock::ClockInstant,
    drop_shadow::DropShadow,
    lane::{LanePlugin, LaneVisual},
    shader::{ADDITIVE_BLENDING, seed_from_entity},
};
use weaponry::{lob::{Lob, LobArc, LobArcProgress, LobLanded}, prelude::*};

use crate::{
    blast::{Blast, BlastCell, BlastHitManager},
    lob::{LoadLobExtension, LobBody, SaveLobExtension, advance_lobs},
};

pub(crate) struct FireballPlugin;
impl Plugin for FireballPlugin {
    fn build(&self, app: &mut App) {
        app
            .add_plugins((
                LanePlugin::<FireballBody>::default(),
                LanePlugin::<FireballGlow>::default(),
                LanePlugin::<FireballImpact>::default(),
            ))
            .add_systems(Update, update_fireball_visuals.after(advance_lobs))
            .add_observer(on_builder_add_spawn_fireball)
            .add_observer(on_lob_landed_detonate_fireball)
            .add_systems(CollectSave, collect_fireballs)
            .register_loader(MapLoadingStage::SpawnMapElements, "fireballs", load_fireballs);
    }
}

// Blast area, in steps from the landing cell
/// Inner blast radius in steps (full damage).
/// Paired with `MAIN_HALF_SIZE` in `assets/shaders/weaponry/fireball_impact.wesl`.
const MAIN_BLAST_REACH: u8 = 1;
/// Outer blast radius in steps (ring damage).
/// Paired with `AREA_RADIUS` in `assets/shaders/weaponry/fireball_impact.wesl`.
const RING_REACH: u8 = 2;
/// Damage multiplier for cells hit at `RING_REACH`.
const RING_DAMAGE_SHARE: f32 = 0.4;

// Body
const CORE_RADIUS: f32 = 0.56 * CELL_SIZE;
/// Body mesh half-size along the flight, in core radii. Leaves room for the tail.
const BODY_HALF_LENGTH: f32 = 4.5;
/// Body mesh half-size across the flight, in core radii.
const BODY_HALF_WIDTH: f32 = 2.5;
/// Tail response to speed: 0 keeps a fixed length; 1 makes length proportional to speed.
const TAIL_SPEED_RESPONSE: f32 = 0.7;
const SHADOW_DIAMETER: f32 = 2.6 * CORE_RADIUS;

// Light pool
const GLOW_INTENSITY: f32 = 0.4;
/// Pool radius on the ground.
const GLOW_RADIUS: f32 = 2. * CORE_RADIUS;
/// Pool radius gained per cell of height, relative to `GLOW_RADIUS`.
const GLOW_SPREAD: f32 = 0.3;
/// Dimming coefficient: intensity is divided by `1 + GLOW_FALLOFF * height_in_cells²`.
const GLOW_FALLOFF: f32 = 0.08;
/// Glow mesh half-size, in pool radii. Leaves room for the glow to fade before the edge.
const GLOW_MESH_HALF_SIZE: f32 = 2.;

// Impact
/// Quad extent in cells beyond the landing cell (7x7 total). Paired with `MESH_HALF_SIZE` in
/// `assets/shaders/weaponry/fireball_impact.wesl`.
const IMPACT_REACH: i32 = 3;
/// Long enough for the blast's smoke to clear.
const BLAST_DURATION: f32 = 1.2;
const SCORCH_DURATION: f32 = 3.;

/// The fireball's light pool on the ground, drawn additively by the paint lane. Child of a
/// `Fireball`. Field order and types mirror `Instance` in `assets/shaders/weaponry/fireball_glow.wesl`.
#[derive(Component, ShaderType, TypePath, Clone, PartialEq)]
#[require(ZDepth::FireballGlow, DrawLevel::OnGround)]
struct FireballGlow {
    intensity: f32,
    /// Mesh half-size, in pool radii.
    mesh_half_size: f32,
}
impl LaneVisual for FireballGlow {
    const LABEL: &'static str = "fireball_glow";
    const LOOK: &'static str = "shaders::weaponry::fireball_glow";

    fn quad_size() -> Vec2 {
        Vec2::splat(2. * GLOW_MESH_HALF_SIZE * GLOW_RADIUS)
    }

    fn blend() -> BlendState {
        ADDITIVE_BLENDING
    }
}

/// The fireball's flying body. Child of a `Fireball`, scaled and offset by the lob. Field order and
/// types mirror `Instance` in `assets/shaders/weaponry/fireball.wesl`.
#[derive(Component, ShaderType, TypePath, Clone, PartialEq)]
struct FireballBody {
    seed: f32,
    /// Tail length multiplier, following the fireball's speed.
    tail: f32,
    /// Mesh half-size along the flight, in core radii.
    half_length: f32,
    /// Mesh half-size across the flight, in core radii.
    half_width: f32,
}
impl LaneVisual for FireballBody {
    const LABEL: &'static str = "fireball_body";
    const LOOK: &'static str = "shaders::weaponry::fireball";

    fn quad_size() -> Vec2 {
        Vec2::new(2. * BODY_HALF_LENGTH * CORE_RADIUS, 2. * BODY_HALF_WIDTH * CORE_RADIUS)
    }
}

/// Mirrors the `layer` values in `assets/shaders/weaponry/fireball_impact.wesl`.
#[derive(Clone, Copy)]
enum ImpactLayer {
    Blast = 0,
    Scorch = 1,
}

/// One impact layer on a temporary entity, despawned when its `TimeBound` expires.
/// Field order and types mirror `Instance` in `assets/shaders/weaponry/fireball_impact.wesl`.
#[derive(Component, ShaderType, TypePath, Clone)]
#[require(MapBound)]
struct FireballImpact {
    /// Game clock instant of the impact.
    start: ClockInstant,
    seed: f32,
    /// An `ImpactLayer`.
    layer: u32,
    duration: f32,
    reach_masks: ImpactReachMasks,
}
impl LaneVisual for FireballImpact {
    const LABEL: &'static str = "fireball_impact";
    const LOOK: &'static str = "shaders::weaponry::fireball_impact";

    fn quad_size() -> Vec2 {
        Vec2::splat((2 * IMPACT_REACH + 1) as f32 * CELL_SIZE)
    }

    /// Combines additive light with alpha-blended fire and smoke.
    fn blend() -> BlendState {
        BlendState::PREMULTIPLIED_ALPHA_BLENDING
    }
}
impl FireballImpact {
    fn new(layer: ImpactLayer, time_bound: TimeBound, seed: f32, reach_masks: ImpactReachMasks) -> Self {
        Self {
            start: ClockInstant::at(time_bound.starts_at),
            seed,
            layer: layer as u32,
            duration: time_bound.duration() as f32,
            reach_masks,
        }
    }
}

/// 25-bit cell masks for the 5x5 blast area, indexed row-major from the bottom-left corner.
/// Field order and types mirror `ReachMasks` in `assets/shaders/weaponry/fireball_impact.wesl`.
#[derive(ShaderType, Clone, Copy, Default)]
struct ImpactReachMasks {
    /// Cells reached by the blast (`Passed` or `Hit`).
    passed: u32,
    /// Reached cells with a floor at the blast's level (`Hit`).
    hit: u32,
    /// Unblocked cells at the blast's level (`!Blocked`).
    open: u32,
}
impl ImpactReachMasks {
    fn from_blast(blast: &Blast) -> Self {
        let mut masks = Self::default();
        for (index, (_, cell)) in blast.cells().enumerate() {
            let bit = 1 << index;
            match cell {
                BlastCell::Blocked => {}
                BlastCell::OutOfReach => masks.open |= bit,
                BlastCell::Passed { .. } => { masks.open |= bit; masks.passed |= bit; }
                BlastCell::Hit { .. } => { masks.open |= bit; masks.passed |= bit; masks.hit |= bit; }
            }
        }
        masks
    }
}

#[log_tags(Tag::GameSave)]
fn collect_fireballs(
    fireballs: Query<(Entity, &LobArc, &LobArcProgress, &AttackDamage), With<Fireball>>,
    mut save: SaveWriter,
) {
    if fireballs.is_empty() { return; }

    #[debug_dev("Saving {} fireballs", rows.len())]
    let rows: Vec<(u32, f32, LobArc, LobArcProgress)> = fireballs
        .iter()
        .map(|(entity, arc, progress, damage)| (entity.index_u32(), damage.get(), *arc, *progress))
        .collect();
    save.submit(move |ctx| {
        for (id, damage, arc, progress) in rows {
            ctx.register_entity(id)?;
            ctx.tx.execute(
                "INSERT OR REPLACE INTO fireballs (id, damage) VALUES (?1, ?2)",
                rusqlite::params![id, damage],
            )?;
            ctx.save_lob(id, arc, progress)?;
        }
        Ok(())
    });
}

fn load_fireballs(ctx: &mut LoadContext) -> LoadResult {
    ctx.for_each_entity("SELECT id, damage FROM fireballs", |ctx, old_id, entity, row| {
        let damage: f32 = row.get(1)?;
        let (arc, progress) = ctx.lob(old_id)?;
        let builder = BuilderFireball::new(arc, AttackDamage::new(damage)).with_progress(progress);
        ctx.insert(entity, builder);
        Ok(())
    })
}

fn on_builder_add_spawn_fireball(
    trigger: On<Add<BuilderFireball>>,
    mut commands: Commands,
    builders: Query<&BuilderFireball>,
) {
    let entity = trigger.entity;
    let Ok(builder) = builders.get(entity) else { return; };

    let arc = builder.arc;
    let body = FireballBody {
        seed: seed_from_entity(entity),
        tail: 1.,
        half_length: BODY_HALF_LENGTH,
        half_width: BODY_HALF_WIDTH,
    };
    commands.entity(entity)
        .remove::<BuilderFireball>()
        .insert_some(builder.shooter.map(ProjectileOf))
        .insert((
            Fireball,
            arc,
            builder.progress,
            Lob,
            builder.damage,
            children![
                (
                    LobBody,
                    body,
                    // Override the generic lob layer to keep fireball bodies adjacent for batching.
                    ZDepth::FireballBody,
                    Transform::from_rotation(Quat::from_rotation_z(arc.heading().to_angle())),
                ),
                FireballGlow { intensity: GLOW_INTENSITY, mesh_half_size: GLOW_MESH_HALF_SIZE },
                DropShadow::new(SHADOW_DIAMETER, arc.target),
            ],
        ));
}

fn on_lob_landed_detonate_fireball(
    trigger: On<LobLanded>,
    mut commands: Commands,
    mut damage_messages: MessageWriter<DamageMessage>,
    clock: Res<GameClock>,
    blast_hit_manager: BlastHitManager,
    fireballs: Query<(&LobArc, &AttackDamage), With<Fireball>>,
) {
    let entity = trigger.0;
    let Ok((arc, attack_damage)) = fireballs.get(entity) else { return; };
    commands.entity(entity).despawn();

    let landing_coords = GridCoords::from_world_position(arc.target);
    let Some(blast) = blast_hit_manager.go_off(landing_coords, RING_REACH) else { return; };
    let reach_masks = ImpactReachMasks::from_blast(&blast);
    let seed = seed_from_entity(entity);
    let (blast_time, scorch_time) = (TimeBound::lasting(&clock, BLAST_DURATION), TimeBound::lasting(&clock, SCORCH_DURATION));
    let transform = Transform::from_translation(landing_coords.world_center().extend(0.));
    commands.spawn((FireballImpact::new(ImpactLayer::Blast, blast_time, seed, reach_masks), blast_time, transform, ZDepth::FireballBlast, DrawLevel::OnElevation));
    commands.spawn((FireballImpact::new(ImpactLayer::Scorch, scorch_time, seed, reach_masks), scorch_time, transform, ZDepth::FireballScorch, DrawLevel::from(blast.position.level)));
    for hit in blast.hits() {
        let damage = if hit.step <= MAIN_BLAST_REACH { attack_damage.get() } else { attack_damage.get() * RING_DAMAGE_SHARE };
        for &wisp in hit.wisps {
            damage_messages.write(DamageMessage { target: wisp, amount: damage });
        }
    }
}

fn update_fireball_visuals(
    fireballs: Query<(&LobArc, &LobArcProgress), With<Fireball>>,
    mut bodies: Query<(&ChildOf, &mut FireballBody)>,
    mut glows: Query<(&ChildOf, &mut Transform, &mut FireballGlow)>,
) {
    for (child_of, mut body) in bodies.iter_mut() {
        let Ok((arc, progress)) = fireballs.get(child_of.parent()) else { continue; };
        let tail = 1. + (arc.speed_ratio(progress.0) - 1.) * TAIL_SPEED_RESPONSE;
        body.set_if_neq(FireballBody { tail, ..*body });
    }
    for (child_of, mut transform, mut glow) in glows.iter_mut() {
        let Ok((arc, progress)) = fireballs.get(child_of.parent()) else { continue; };
        let height_in_cells = arc.sample(progress.0).height / CELL_SIZE;
        transform.scale = Vec3::splat(1. + GLOW_SPREAD * height_in_cells);
        let intensity = GLOW_INTENSITY / (1. + height_in_cells * height_in_cells * GLOW_FALLOFF);
        glow.set_if_neq(FireballGlow { intensity, ..*glow });
    }
}
