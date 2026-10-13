//! # Fireball
//!
//! Blaze Mortar projectile. On landing, it deals full damage to wisps in a 3x3 area centered on
//! the landing cell and reduced damage in the surrounding one-cell ring, for a 5x5 footprint.
//! The damage zones do not overlap.
//!
//! ## Rendering
//!
//! The body faces the flight direction. Its tail shortens as it slows near the apex and lengthens
//! as it falls. An additive glow beneath the projectile spreads and dims with height.
//!
//! One impact shader draws two layers: a blast above wisps and a longer-lived scorch mark below
//! them. A mask sampled from the obstacle grid at impact keeps ground fire off walls, buildings,
//! and dark ore.

use bevy::{
    mesh::MeshVertexBufferLayoutRef,
    prelude::*,
    reflect::TypePath,
    render::render_resource::{
        AsBindGroup, BlendState, RenderPipelineDescriptor, ShaderType,
        SpecializedMeshPipelineError,
    },
    shader::ShaderRef,
    sprite_render::{AlphaMode2d, Material2d, Material2dKey, Material2dPipeline, Material2dPlugin, MeshMaterial2d},
};

use alteration::modifiers::prelude::AttackDamage;
use game_core::prelude::{CELL_SIZE, DamageMessage, GridCoords, GridImprint, MapBound, OptionalCommands, Property, ZDepth};
use grids::{obstacles::ObstacleGrid, wisps::WispsGrid};
use logging::prelude::*;
use persistence::{prelude::*, rusqlite};
use states::prelude::{GameState, MapLoadingStage};
use visuals::shader::{ADDITIVE_BLENDING, seed_from_entity, set_blend};
use weaponry::{lob::Lob, prelude::*};
use wisps::prelude::Wisp;

use crate::lob::{LoadLobExtension, LobBody, LobDiscImage, SaveLobExtension, advance_lobs};

pub(crate) struct FireballPlugin;
impl Plugin for FireballPlugin {
    fn build(&self, app: &mut App) {
        app
            .add_plugins((
                Material2dPlugin::<FireballMaterial>::default(),
                Material2dPlugin::<FireballGlowMaterial>::default(),
                Material2dPlugin::<FireballImpactMaterial>::default(),
            ))
            .init_resource::<FireballMeshes>()
            .add_systems(Update, (
                fireball_hit_system.after(advance_lobs).run_if(in_state(GameState::Running)),
                advance_fireball_impacts.run_if(in_state(GameState::Running)),
                update_fireball_visuals,
            ))
            .add_observer(on_builder_add_spawn_fireball)
            .add_observer(on_builder_add_spawn_fireball_impact)
            .add_systems(CollectSave, collect_fireballs)
            .register_loader(MapLoadingStage::SpawnMapElements, "fireballs", load_fireballs);
    }
}

// Blast footprint, in cells from the landing cell
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
/// Visual reach in cells beyond the landing cell. Limited to 3 so the square raised-cell mask
/// fits in 64 bits: `(2 * reach + 1)² <= 64`.
const IMPACT_REACH: i32 = 3;
const _: () = assert!((2 * IMPACT_REACH + 1) * (2 * IMPACT_REACH + 1) <= 64);
/// Long enough for the blast's smoke to clear.
const BLAST_DURATION: f32 = 1.2;
const SCORCH_DURATION: f32 = 3.;

/// The fireball's light pool on the ground. Child of a `Fireball`.
#[derive(Component)]
struct FireballGlow;

/// Spawns the impact of a fireball that landed on this cell.
#[derive(Component)]
struct BuilderFireballImpact(GridCoords);

/// One layer of a fireball impact. Despawns once it has played out.
#[derive(Component)]
#[require(MapBound)]
struct FireballImpact {
    /// Seconds since impact.
    age: f32,
    duration: f32,
}
impl FireballImpact {
    fn new(duration: f32) -> Self {
        Self { age: 0., duration }
    }

    fn age(&self) -> f32 { self.age }

    fn advance(&mut self, seconds: f32) {
        self.age += seconds;
    }

    fn is_played_out(&self) -> bool {
        self.age >= self.duration
    }
}

/// Mirrors the `layer` values in `assets/shaders/weaponry/fireball_impact.wesl`.
#[derive(Clone, Copy)]
enum ImpactLayer {
    Blast = 0,
    Scorch = 1,
}

/// Meshes shared by every fireball.
#[derive(Resource)]
struct FireballMeshes {
    body: Handle<Mesh>,
    glow: Handle<Mesh>,
    impact: Handle<Mesh>,
}
impl FromWorld for FireballMeshes {
    fn from_world(world: &mut World) -> Self {
        let mut meshes = world.resource_mut::<Assets<Mesh>>();
        Self {
            body: meshes.add(Rectangle::new(2. * BODY_HALF_LENGTH * CORE_RADIUS, 2. * BODY_HALF_WIDTH * CORE_RADIUS)),
            glow: meshes.add(Rectangle::from_length(2. * GLOW_MESH_HALF_SIZE * GLOW_RADIUS)),
            impact: meshes.add(Rectangle::from_length((2 * IMPACT_REACH + 1) as f32 * CELL_SIZE)),
        }
    }
}

/// Field order and types mirror `Fireball` in `assets/shaders/weaponry/fireball.wesl`.
#[derive(ShaderType, Clone, Copy, Debug, Default, PartialEq)]
struct FireballUniform {
    seed: f32,
    tail: f32,
    half_length: f32,
    half_width: f32,
}

#[derive(Asset, AsBindGroup, TypePath, Clone, Copy, Debug, Default)]
struct FireballMaterial {
    #[uniform(0)]
    fireball: FireballUniform,
}
impl Material2d for FireballMaterial {
    fn fragment_shader() -> ShaderRef {
        "shaders/weaponry/fireball.wesl".into()
    }
    fn alpha_mode(&self) -> AlphaMode2d {
        AlphaMode2d::Blend
    }
}

/// Field order and types mirror `FireballImpact` in `assets/shaders/weaponry/fireball_impact.wesl`.
#[derive(ShaderType, Clone, Copy, Debug, Default, PartialEq)]
struct FireballImpactUniform {
    age: f32,
    seed: f32,
    layer: u32,
    duration: f32,
    reach: u32,
    main_half_size: f32,
    ring_half_size: f32,
    raised_low: u32,
    raised_high: u32,
}

/// Uses premultiplied blending to combine additive light with alpha-blended fire and smoke.
#[derive(Asset, AsBindGroup, TypePath, Clone, Copy, Debug, Default)]
struct FireballImpactMaterial {
    #[uniform(0)]
    impact: FireballImpactUniform,
}
impl Material2d for FireballImpactMaterial {
    fn fragment_shader() -> ShaderRef {
        "shaders/weaponry/fireball_impact.wesl".into()
    }
    fn alpha_mode(&self) -> AlphaMode2d {
        AlphaMode2d::Blend
    }
    fn specialize(
        _pipeline: &Material2dPipeline,
        descriptor: &mut RenderPipelineDescriptor,
        _layout: &MeshVertexBufferLayoutRef,
        _key: Material2dKey<Self>,
    ) -> Result<(), SpecializedMeshPipelineError> {
        set_blend(descriptor, BlendState::PREMULTIPLIED_ALPHA_BLENDING);
        Ok(())
    }
}

/// Field order and types mirror `FireballGlow` in `assets/shaders/weaponry/fireball_glow.wesl`.
#[derive(ShaderType, Clone, Copy, Debug, Default, PartialEq)]
struct FireballGlowUniform {
    intensity: f32,
    mesh_half_size: f32,
}

/// Adds glow color to the scene without changing destination alpha.
#[derive(Asset, AsBindGroup, TypePath, Clone, Copy, Debug, Default)]
struct FireballGlowMaterial {
    #[uniform(0)]
    glow: FireballGlowUniform,
}
impl Material2d for FireballGlowMaterial {
    fn fragment_shader() -> ShaderRef {
        "shaders/weaponry/fireball_glow.wesl".into()
    }
    fn alpha_mode(&self) -> AlphaMode2d {
        AlphaMode2d::Blend
    }
    fn specialize(
        _pipeline: &Material2dPipeline,
        descriptor: &mut RenderPipelineDescriptor,
        _layout: &MeshVertexBufferLayoutRef,
        _key: Material2dKey<Self>,
    ) -> Result<(), SpecializedMeshPipelineError> {
        set_blend(descriptor, ADDITIVE_BLENDING);
        Ok(())
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
    mut fireball_materials: ResMut<Assets<FireballMaterial>>,
    mut glow_materials: ResMut<Assets<FireballGlowMaterial>>,
    meshes: Res<FireballMeshes>,
    disc_image: Res<LobDiscImage>,
    builders: Query<&BuilderFireball>,
) {
    let entity = trigger.entity;
    let Ok(builder) = builders.get(entity) else { return; };

    let lob = builder.lob;
    let body_material = FireballMaterial {
        fireball: FireballUniform {
            seed: seed_from_entity(entity),
            tail: 1.,
            half_length: BODY_HALF_LENGTH,
            half_width: BODY_HALF_WIDTH,
        },
    };
    let glow_material = FireballGlowMaterial {
        glow: FireballGlowUniform { intensity: GLOW_INTENSITY, mesh_half_size: GLOW_MESH_HALF_SIZE },
    };
    commands.entity(entity)
        .remove::<BuilderFireball>()
        .insert_some(builder.shooter.map(ProjectileOf))
        .insert((
            Transform::from_translation(lob.current().ground_position.extend(0.)),
            Fireball,
            lob,
            builder.damage,
            children![
                (
                    LobBody,
                    Mesh2d(meshes.body.clone()),
                    MeshMaterial2d(fireball_materials.add(body_material)),
                    Transform::from_rotation(Quat::from_rotation_z(lob.heading().to_angle())),
                    ZDepth::PROJECTILE,
                ),
                (
                    FireballGlow,
                    Mesh2d(meshes.glow.clone()),
                    MeshMaterial2d(glow_materials.add(glow_material)),
                    ZDepth::GROUND_EFFECT,
                ),
                disc_image.make_shadow(SHADOW_DIAMETER),
            ],
        ));
}

fn fireball_hit_system(
    mut commands: Commands,
    mut damage_messages: MessageWriter<DamageMessage>,
    wisps_grid: Res<WispsGrid>,
    fireballs: Query<(Entity, &Lob, &AttackDamage), With<Fireball>>,
    wisps: Query<(), With<Wisp>>,
) {
    for (entity, lob, attack_damage) in fireballs.iter() {
        if !lob.is_landed() { continue; }

        let landing_coords = GridCoords::from_world_vec2(lob.target());
        commands.spawn(BuilderFireballImpact(landing_coords));
        for dx in -RING_REACH..=RING_REACH {
            for dy in -RING_REACH..=RING_REACH {
                let blast_zone_coords = landing_coords.shifted((dx, dy));
                if !blast_zone_coords.are_in_bounds(wisps_grid.bounds) { continue; }

                let in_main_blast = dx.abs().max(dy.abs()) <= MAIN_BLAST_REACH;
                let damage = if in_main_blast { attack_damage.get() } else { attack_damage.get() * RING_DAMAGE_SHARE };
                for wisp in &wisps_grid[blast_zone_coords] {
                    if !wisps.contains(*wisp) { continue; } // Grid entries may not yet be visible to the query.
                    damage_messages.write(DamageMessage { target: *wisp, amount: damage });
                }
            }
        }
        commands.entity(entity).despawn();
    }
}

fn update_fireball_visuals(
    mut fireball_materials: ResMut<Assets<FireballMaterial>>,
    mut glow_materials: ResMut<Assets<FireballGlowMaterial>>,
    lobs: Query<&Lob, With<Fireball>>,
    bodies: Query<(&ChildOf, &MeshMaterial2d<FireballMaterial>)>,
    mut glows: Query<(&ChildOf, &mut Transform, &MeshMaterial2d<FireballGlowMaterial>), With<FireballGlow>>,
) {
    for (child_of, material) in bodies.iter() {
        let Ok(lob) = lobs.get(child_of.parent()) else { continue; };
        let tail = 1. + (lob.speed_ratio() - 1.) * TAIL_SPEED_RESPONSE;
        let Some(mut current) = fireball_materials.get_mut(&material.0) else { continue; };
        if current.fireball.tail != tail {
            current.fireball.tail = tail;
        }
    }
    for (child_of, mut transform, material) in glows.iter_mut() {
        let Ok(lob) = lobs.get(child_of.parent()) else { continue; };
        let height_in_cells = lob.current().height / CELL_SIZE;
        transform.scale = Vec3::splat(1. + GLOW_SPREAD * height_in_cells);
        let intensity = GLOW_INTENSITY / (1. + height_in_cells * height_in_cells * GLOW_FALLOFF);
        let Some(mut current) = glow_materials.get_mut(&material.0) else { continue; };
        if current.glow.intensity != intensity {
            current.glow.intensity = intensity;
        }
    }
}

fn on_builder_add_spawn_fireball_impact(
    trigger: On<Add<BuilderFireballImpact>>,
    mut commands: Commands,
    mut impact_materials: ResMut<Assets<FireballImpactMaterial>>,
    meshes: Res<FireballMeshes>,
    obstacle_grid: Res<ObstacleGrid>,
    builders: Query<&BuilderFireballImpact>,
) {
    let entity = trigger.entity;
    let Ok(builder) = builders.get(entity) else { return; };
    let landing_coords = builder.0;

    // One bit per cell around the landing cell, row by row. Cells off the map count as raised.
    let row_length = 2 * IMPACT_REACH + 1;
    let mut raised: u64 = 0;
    for dy in -IMPACT_REACH..=IMPACT_REACH {
        for dx in -IMPACT_REACH..=IMPACT_REACH {
            let coords = landing_coords.shifted((dx, dy));
            let is_raised = !coords.are_in_bounds(obstacle_grid.bounds) || {
                let field = &obstacle_grid[coords];
                field.has_structure() || field.has_dark_ore()
            };
            if is_raised {
                raised |= 1 << ((dy + IMPACT_REACH) * row_length + dx + IMPACT_REACH);
            }
        }
    }
    let material = |layer: ImpactLayer, duration: f32| FireballImpactMaterial {
        impact: FireballImpactUniform {
            age: 0.,
            seed: seed_from_entity(entity),
            layer: layer as u32,
            duration,
            reach: IMPACT_REACH as u32,
            main_half_size: MAIN_BLAST_REACH as f32 + 0.5,
            ring_half_size: RING_REACH as f32 + 0.5,
            raised_low: raised as u32,
            raised_high: (raised >> 32) as u32,
        },
    };
    let transform = Transform::from_translation(landing_coords.to_world_position_centered(GridImprint::default()).extend(0.));

    commands.entity(entity)
        .remove::<BuilderFireballImpact>()
        .insert((
            FireballImpact::new(BLAST_DURATION),
            Mesh2d(meshes.impact.clone()),
            MeshMaterial2d(impact_materials.add(material(ImpactLayer::Blast, BLAST_DURATION))),
            transform,
            ZDepth::GROUND_EFFECT,
        ));
    commands.spawn((
        FireballImpact::new(SCORCH_DURATION),
        Mesh2d(meshes.impact.clone()),
        MeshMaterial2d(impact_materials.add(material(ImpactLayer::Scorch, SCORCH_DURATION))),
        transform,
        ZDepth::GROUND_DECAL,
    ));
}

fn advance_fireball_impacts(
    mut commands: Commands,
    time: Res<Time>,
    mut impact_materials: ResMut<Assets<FireballImpactMaterial>>,
    mut impacts: Query<(Entity, &mut FireballImpact, &MeshMaterial2d<FireballImpactMaterial>)>,
) {
    for (entity, mut impact, material) in impacts.iter_mut() {
        impact.advance(time.delta_secs());
        if impact.is_played_out() {
            commands.entity(entity).despawn();
            continue;
        }
        let Some(mut current) = impact_materials.get_mut(&material.0) else { continue; };
        current.impact.age = impact.age();
    }
}
