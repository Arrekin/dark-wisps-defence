//! # Rocket
//!
//! Homing projectile fired by the Rocket Launcher. Tracks a target wisp while it remains at the
//! top of its cell, retargeting to another top-level wisp if its target is lost or enters a
//! tunnel/underpass (or flying straight if none exist).
//!
//! Passing within `ROCKET_HIT_DISTANCE` of the target detonates a 1-step (3x3) blast via
//! [`BlastHitManager`] at the target wisp's cell, spawning an explosion and damaging wisps in every
//! hit cell.

use bevy::{
    ecs::query::QueryData,
    prelude::*,
    sprite::Anchor,
};

use alteration::modifiers::prelude::AttackDamage;
use game_core::{
    math::angle_difference,
    prelude::{DamageMessage, GridArea, GridCoords, Property},
};
use indexes::{prelude::ObstacleGrid, wisps::WispsGrid};
use logging::prelude::*;
use persistence::{prelude::*, rusqlite};
use states::prelude::{GameState, MapLoadingStage};
use visuals::prelude::BuilderExplosion;
use weaponry::prelude::*;
use wisps::prelude::Wisp;

use crate::blast::BlastHitManager;

pub(crate) struct RocketPlugin;
impl Plugin for RocketPlugin {
    fn build(&self, app: &mut App) {
        app
            .add_systems(Update, (
                blink_rocket_exhausts,
                (
                    move_rockets,
                    resolve_rocket_hits,
                ).chain().run_if(in_state(GameState::Running)),
            ))
            .add_observer(on_builder_add_spawn_rocket)
            .add_systems(CollectSave, collect_rockets)
            .register_loader(MapLoadingStage::SpawnMapElements, "rockets", load_rockets);
    }
}

const ROCKET_BASE_IMAGE: &str = "projectiles/rocket.png";
const ROCKET_EXHAUST_IMAGE: &str = "projectiles/rocket_exhaust.png";

// Flight tuning
const ROCKET_SPEED: f32 = 400.0;
/// Radians per second.
const ROCKET_TURN_SPEED: f32 = 1.5;
const ROCKET_HIT_DISTANCE: f32 = 6.0;

// Blast area, in steps from the struck wisp's cell
const BLAST_REACH: u8 = 1;

#[log_tags(Tag::GameSave)]
fn collect_rockets(
    rockets: Query<(Entity, &Rocket, &Transform, &RocketTarget, &AttackDamage)>,
    mut save: SaveWriter,
) {
    if rockets.is_empty() { return; }

    #[debug_dev("Saving {} rockets", rows.len())]
    let rows: Vec<(u32, Vec2, Option<u32>, f32, f32)> = rockets
        .iter()
        .map(|(entity, rocket, transform, target, damage)| {
            (
                entity.index_u32(),
                transform.translation.xy(),
                target.0.map(|wisp| wisp.index_u32()),
                rocket.heading,
                damage.get(),
            )
        })
        .collect();
    save.submit(move |ctx| {
        for (id, position, target_wisp_id, heading, damage) in rows {
            ctx.register_entity(id)?;
            ctx.save_world_position(id, position)?;
            ctx.tx.execute(
                "INSERT OR REPLACE INTO rockets (id, target_wisp_id, heading, damage) VALUES (?1, ?2, ?3, ?4)",
                rusqlite::params![id, target_wisp_id, heading, damage],
            )?;
        }
        Ok(())
    });
}

fn load_rockets(ctx: &mut LoadContext) -> LoadResult {
    ctx.for_each_entity("SELECT id, target_wisp_id, heading, damage FROM rockets", |ctx, old_id, entity, row| {
        let target_wisp_old_id: Option<u32> = row.get(1)?;
        let heading = row.get(2)?;
        let damage = AttackDamage::new(row.get(3)?);
        let world_position = ctx.world_position(old_id)?;
        let builder = BuilderRocket::new(world_position, heading, damage)
            .with_target_wisp(ctx.optional_entity(target_wisp_old_id).unwrap_or_default());
        ctx.insert(entity, builder);
        Ok(())
    })
}

fn on_builder_add_spawn_rocket(
    trigger: On<Add<BuilderRocket>>,
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    builders: Query<&BuilderRocket>,
) {
    let entity = trigger.entity;
    let Ok(builder) = builders.get(entity) else { return; };

    commands.entity(entity)
        .remove::<BuilderRocket>()
        .insert((
            Sprite {
                image: asset_server.load(ROCKET_BASE_IMAGE),
                custom_size: Some(Vec2::new(40.0, 20.0)),
                ..default()
            },
            Transform {
                translation: builder.world_position.extend(0.),
                rotation: Quat::from_rotation_z(builder.heading),
                ..default()
            },
            Rocket { heading: builder.heading },
            RocketTarget(builder.target_wisp),
            builder.damage,
            // Exhaust
            children![(
                Sprite {
                    image: asset_server.load(ROCKET_EXHAUST_IMAGE),
                    custom_size: Some(Vec2::new(20.0, 12.5)),
                    ..default()
                },
                Anchor(Vec2::new(0.9, 0.)),
                RocketExhaust,
            )]
        ));
}

#[derive(QueryData)]
struct RocketTargetWisp {
    entity: Entity,
    transform: &'static Transform,
    area: &'static GridArea,
}

fn move_rockets(
    time: Res<Time>,
    obstacle_grid: Res<ObstacleGrid>,
    mut rockets: Query<(&mut Rocket, &mut Transform, &mut RocketTarget)>,
    wisps: Query<RocketTargetWisp, (With<Wisp>, Without<Rocket>)>,
) {
    let travel = ROCKET_SPEED * time.delta_secs();
    let max_turn = ROCKET_TURN_SPEED * time.delta_secs();
    let at_top = |wisp: &RocketTargetWispItem| obstacle_grid.is_top(wisp.area.origin);
    let mut wisps_iter = wisps.iter();
    for (mut rocket, mut transform, mut target) in rockets.iter_mut() {
        // Retarget if the current target is despawned or no longer at the top of its cell.
        let target_wisp = target.0.and_then(|wisp| wisps.get(wisp).ok()).filter(at_top).or_else(|| {
            let next_wisp = wisps_iter.find(at_top);
            target.0 = next_wisp.as_ref().map(|wisp| wisp.entity);
            next_wisp
        });

        if let Some(wisp) = target_wisp {
            let angle_delta = angle_difference((wisp.transform.translation.xy() - transform.translation.xy()).to_angle(), rocket.heading);
            rocket.heading += angle_delta.clamp(-max_turn, max_turn);
        }

        transform.translation += (Vec2::from_angle(rocket.heading) * travel).extend(0.0);
        transform.rotation = Quat::from_rotation_z(rocket.heading);
    }
}

/// Checks the segment each rocket traveled this frame and detonates if it passed within `ROCKET_HIT_DISTANCE` of its target.
fn resolve_rocket_hits(
    mut commands: Commands,
    mut damage_messages: MessageWriter<DamageMessage>,
    time: Res<Time>,
    wisps_grid: Res<WispsGrid>,
    blast_hit_manager: BlastHitManager,
    rockets: Query<(Entity, &Rocket, &Transform, &RocketTarget, &AttackDamage)>,
    wisps: Query<(&Transform, &GridArea), With<Wisp>>,
) {
    let travel = ROCKET_SPEED * time.delta_secs();
    for (entity, rocket, rocket_transform, target, attack_damage) in rockets.iter() {
        if !wisps_grid.bounds.contains(GridCoords::from_transform(rocket_transform)) {
            commands.entity(entity).despawn();
            continue;
        }

        let Some(Ok((wisp_transform, wisp_area))) = target.0.map(|wisp| wisps.get(wisp)) else { continue; };
        let direction = Vec2::from_angle(rocket.heading);
        let start = rocket_transform.translation.xy() - direction * travel;
        let offset = wisp_transform.translation.xy() - start;
        let along = offset.dot(direction).clamp(0., travel);
        if offset.distance(direction * along) > ROCKET_HIT_DISTANCE { continue; }

        commands.entity(entity).despawn();
        let Some(blast) = blast_hit_manager.go_off(wisp_area.origin.coords, BLAST_REACH) else { continue; };
        for hit in blast.hits() {
            commands.spawn(BuilderExplosion(hit.position));
            for &wisp in hit.wisps {
                damage_messages.write(DamageMessage { target: wisp, amount: attack_damage.get() });
            }
        }
    }
}

fn blink_rocket_exhausts(
    time: Res<Time>,
    mut exhausts: Query<&mut Sprite, With<RocketExhaust>>,
) {
    let alpha = if time.elapsed_secs() % 1. < 0.85 { 1. } else { 0. };
    for mut sprite in exhausts.iter_mut() {
        sprite.color.set_alpha(alpha);
    }
}
