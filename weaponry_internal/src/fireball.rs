//! # Fireball
//!
//! Blaze Mortar projectile. On landing, it deals full damage to wisps in a 3x3 area centered on
//! the landing cell and reduced damage in the surrounding one-cell ring, for a 5x5 area.
//! The damage zones do not overlap.
//!
//! ## Rendering
//!
//! The body faces the flight direction. Its tail shortens as it slows near the apex and lengthens
//! as it falls. An additive glow beneath the projectile spreads and dims with height.
//!
//! One impact shader draws two layers: a blast above wisps and a longer-lived scorch mark below
//! them. Shared obstacle data masks ground fire on walls, buildings and dark ore.
//! The mask reflects changes to those cells throughout the impact.
//!
//! Body, glow and impact use dedicated lane layers for batching. Impact instances store a start
//! time and duration; the shader derives age from the game clock without per-frame instance updates.

use bevy::{
    prelude::*,
    reflect::TypePath,
    render::render_resource::{BlendState, ShaderType},
};

use alteration::modifiers::prelude::AttackDamage;
use game_core::prelude::{CELL_SIZE, DamageMessage, GridArea, GridCoords, GridImprint, MapBound, OptionalCommands, Property, ZDepth};
use grids::wisps::WispsGrid;
use logging::prelude::*;
use persistence::{prelude::*, rusqlite};
use session::{GameClock, TimeBound};
use states::prelude::{GameState, MapLoadingStage};
use visuals::{
    lane::{LanePlugin, LaneVisual},
    shader::{ADDITIVE_BLENDING, seed_from_entity},
};
use weaponry::{lob::{Lob, LobInFlight}, prelude::*};
use wisps::prelude::Wisp;

use crate::lob::{LoadLobExtension, LobBody, LobDiscImage, SaveLobExtension, advance_lobs};

pub(crate) struct FireballPlugin;
impl Plugin for FireballPlugin {
    fn build(&self, app: &mut App) {
        app
            .add_plugins((
                LanePlugin::<FireballBody>::default(),
                LanePlugin::<FireballGlow>::default(),
                LanePlugin::<FireballImpact>::default(),
            ))
            .add_systems(Update, (
                fireball_hit_system.after(advance_lobs).run_if(in_state(GameState::Running)),
                update_fireball_visuals.after(advance_lobs),
            ))
            .add_observer(on_builder_add_spawn_fireball)
            .add_systems(CollectSave, collect_fireballs)
            .register_loader(MapLoadingStage::SpawnMapElements, "fireballs", load_fireballs);
    }
}

// Blast area, in cells from the landing cell
const MAIN_BLAST_REACH: i32 = 1;
const RING_REACH: i32 = 2;
/// Share of the fireball's damage dealt to cells in the ring.
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
/// Visual reach in cells beyond the landing cell.
const IMPACT_REACH: i32 = 3;
/// Long enough for the blast's smoke to clear.
const BLAST_DURATION: f32 = 1.2;
const SCORCH_DURATION: f32 = 3.;

/// The fireball's light pool on the ground, drawn additively by the paint lane. Child of a
/// `Fireball`. Field order and types mirror `Instance` in `assets/shaders/weaponry/fireball_glow.wesl`.
#[derive(Component, ShaderType, TypePath, Clone, PartialEq)]
#[require(ZDepth::FireballGlow)]
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
/// Field order and types mirror `Instance` in
/// `assets/shaders/weaponry/fireball_impact.wesl`.
#[derive(Component, ShaderType, TypePath, Clone)]
#[require(MapBound)]
struct FireballImpact {
    /// Wrapped game time of the impact.
    start_time: f32,
    seed: f32,
    /// An `ImpactLayer`.
    layer: u32,
    duration: f32,
    /// Mesh extent in cells beyond the landing cell.
    reach: u32,
    main_half_size: f32,
    ring_half_size: f32,
    landing_cell: IVec2,
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
    fn new(layer: ImpactLayer, time_bound: TimeBound, seed: f32, landing_coords: GridCoords) -> Self {
        Self {
            start_time: GameClock::wrap(time_bound.starts_at),
            seed,
            layer: layer as u32,
            duration: time_bound.duration() as f32,
            reach: IMPACT_REACH as u32,
            main_half_size: MAIN_BLAST_REACH as f32 + 0.5,
            ring_half_size: RING_REACH as f32 + 0.5,
            landing_cell: landing_coords.into(),
        }
    }
}

#[log_tags(Tag::GameSave)]
fn collect_fireballs(
    fireballs: Query<(Entity, &Lob, &AttackDamage), With<Fireball>>,
    mut save: SaveWriter,
) {
    if fireballs.is_empty() { return; }

    #[debug_dev("Saving {} fireballs", rows.len())]
    let rows: Vec<(u32, f32, Lob)> = fireballs
        .iter()
        .map(|(entity, lob, damage)| (entity.index_u32(), damage.get(), *lob))
        .collect();
    save.submit(move |ctx| {
        for (id, damage, lob) in rows {
            ctx.register_entity(id)?;
            ctx.tx.execute(
                "INSERT OR REPLACE INTO fireballs (id, damage) VALUES (?1, ?2)",
                rusqlite::params![id, damage],
            )?;
            ctx.save_lob(id, &lob)?;
        }
        Ok(())
    });
}

fn load_fireballs(ctx: &mut LoadContext) -> LoadResult {
    ctx.for_each_entity("SELECT id, damage FROM fireballs", |ctx, old_id, entity, row| {
        let damage: f32 = row.get(1)?;
        let builder = BuilderFireball::new(ctx.lob(old_id)?, AttackDamage::new(damage));
        ctx.insert(entity, builder);
        Ok(())
    })
}

fn on_builder_add_spawn_fireball(
    trigger: On<Add<BuilderFireball>>,
    mut commands: Commands,
    disc_image: Res<LobDiscImage>,
    builders: Query<&BuilderFireball>,
) {
    let entity = trigger.entity;
    let Ok(builder) = builders.get(entity) else { return; };

    let lob = builder.lob;
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
            lob,
            builder.damage,
            children![
                (
                    LobBody,
                    body,
                    // Override the generic lob layer to keep fireball bodies adjacent for batching.
                    ZDepth::FireballBody,
                    Transform::from_rotation(Quat::from_rotation_z(lob.heading().to_angle())),
                ),
                FireballGlow { intensity: GLOW_INTENSITY, mesh_half_size: GLOW_MESH_HALF_SIZE },
                disc_image.make_shadow(SHADOW_DIAMETER),
            ],
        ));
}

fn fireball_hit_system(
    mut commands: Commands,
    mut damage_messages: MessageWriter<DamageMessage>,
    clock: Res<GameClock>,
    wisps_grid: Res<WispsGrid>,
    fireballs: Query<(Entity, &Lob, &AttackDamage), (With<Fireball>, With<LobInFlight>)>,
    wisps: Query<(), With<Wisp>>,
) {
    for (entity, lob, attack_damage) in fireballs.iter() {
        if !lob.is_landed(clock.now()) { continue; }

        let landing_coords = GridCoords::from_world_position(lob.target());
        let seed = seed_from_entity(entity);
        let (blast, scorch) = (TimeBound::lasting(&clock, BLAST_DURATION), TimeBound::lasting(&clock, SCORCH_DURATION));
        let transform = Transform::from_translation(GridArea::cell(landing_coords).world_center().extend(0.));
        commands.spawn((FireballImpact::new(ImpactLayer::Blast, blast, seed, landing_coords), blast, transform, ZDepth::FireballBlast));
        commands.spawn((FireballImpact::new(ImpactLayer::Scorch, scorch, seed, landing_coords), scorch, transform, ZDepth::FireballScorch));
        let main_blast = GridArea::centered_on(landing_coords, GridImprint::square_of_reach(MAIN_BLAST_REACH));
        let blast_zone = GridArea::centered_on(landing_coords, GridImprint::square_of_reach(RING_REACH));
        for blast_zone_coords in blast_zone.iter_in_bounds(wisps_grid.bounds) {
            let damage = if main_blast.contains(blast_zone_coords) { attack_damage.get() } else { attack_damage.get() * RING_DAMAGE_SHARE };
            for wisp in &wisps_grid[blast_zone_coords] {
                if !wisps.contains(*wisp) { continue; } // Grid entries may not yet be visible to the query.
                damage_messages.write(DamageMessage { target: *wisp, amount: damage });
            }
        }
        commands.entity(entity).despawn();
    }
}

fn update_fireball_visuals(
    clock: Res<GameClock>,
    lobs: Query<&Lob, (With<Fireball>, With<LobInFlight>)>,
    mut bodies: Query<(&ChildOf, &mut FireballBody)>,
    mut glows: Query<(&ChildOf, &mut Transform, &mut FireballGlow)>,
) {
    for (child_of, mut body) in bodies.iter_mut() {
        let Ok(lob) = lobs.get(child_of.parent()) else { continue; };
        let tail = 1. + (lob.speed_ratio(clock.now()) - 1.) * TAIL_SPEED_RESPONSE;
        body.set_if_neq(FireballBody { tail, ..*body });
    }
    for (child_of, mut transform, mut glow) in glows.iter_mut() {
        let Ok(lob) = lobs.get(child_of.parent()) else { continue; };
        let height_in_cells = lob.current(clock.now()).height / CELL_SIZE;
        transform.scale = Vec3::splat(1. + GLOW_SPREAD * height_in_cells);
        let intensity = GLOW_INTENSITY / (1. + height_in_cells * height_in_cells * GLOW_FALLOFF);
        glow.set_if_neq(FireballGlow { intensity, ..*glow });
    }
}
