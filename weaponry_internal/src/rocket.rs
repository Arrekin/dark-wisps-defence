use bevy::{
    prelude::*,
    sprite::Anchor,
};

use alteration::modifiers::prelude::AttackDamage;
use game_core::{
    math::angle_difference,
    prelude::{ALL_DIRECTIONS, DamageMessage, GridCoords, Property},
};
use grids::wisps::WispsGrid;
use logging::prelude::*;
use persistence::{prelude::*, rusqlite};
use states::prelude::{GameState, MapLoadingStage};
use visuals::prelude::BuilderExplosion;
use weaponry::prelude::*;
use wisps::prelude::Wisp;

/// Plugin for the Rocket projectile
pub(crate) struct RocketPlugin;
impl Plugin for RocketPlugin {
    fn build(&self, app: &mut App) {
        app
            .add_systems(Update, (
                exhaust_blinking_system,
                (
                    rocket_move_system,
                    rocket_hit_system,
                ).run_if(in_state(GameState::Running)),
            ))
            .add_observer(on_builder_add_spawn_rocket)
            .add_systems(CollectSave, collect_rockets)
            .register_loader(MapLoadingStage::SpawnMapElements, "rockets", load_rockets);
    }
}

pub(crate) const ROCKET_BASE_IMAGE: &str = "projectiles/rocket.png";
pub(crate) const ROCKET_EXHAUST_IMAGE: &str = "projectiles/rocket_exhaust.png";

// Flight tuning
const ROCKET_SPEED: f32 = 400.0;
/// Radians per second.
const ROCKET_TURN_SPEED: f32 = 1.5;
const ROCKET_HIT_DISTANCE: f32 = 6.0;

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
        let heading: f32 = row.get(2)?;
        let damage: f32 = row.get(3)?;
        let world_position = ctx.world_position(old_id)?;

        let builder = BuilderRocket::new(
            world_position,
            heading,
            AttackDamage::new(damage),
        ).with_target_wisp(ctx.optional_entity(target_wisp_old_id).unwrap_or_default());
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

fn rocket_move_system(
    time: Res<Time>,
    mut rockets: Query<(&mut Rocket, &mut Transform, &mut RocketTarget)>,
    wisps: Query<(Entity, &Transform), (With<Wisp>, Without<Rocket>)>,
) {
    let mut wisps_iter = wisps.iter();
    for (mut rocket, mut transform, mut target) in rockets.iter_mut() {
        let target_wisp = target.0.and_then(|wisp| wisps.get(wisp).ok()).or_else(|| {
            let next_wisp = wisps_iter.next();
            target.0 = next_wisp.map(|(wisp, _)| wisp);
            next_wisp
        });

        // Turn toward the target. Without one the rocket flies straight on.
        if let Some((_, wisp_transform)) = target_wisp {
            let angle_delta = angle_difference((wisp_transform.translation.xy() - transform.translation.xy()).to_angle(), rocket.heading);
            let max_turn = ROCKET_TURN_SPEED * time.delta_secs();
            rocket.heading += angle_delta.clamp(-max_turn, max_turn);
        }

        transform.translation += (Vec2::from_angle(rocket.heading) * time.delta_secs() * ROCKET_SPEED).extend(0.0);
        transform.rotation = Quat::from_rotation_z(rocket.heading);
    }
}

fn rocket_hit_system(
    mut commands: Commands,
    mut damage_messages: MessageWriter<DamageMessage>,
    wisps_grid: Res<WispsGrid>,
    rockets: Query<(Entity, &Transform, &RocketTarget, &AttackDamage), (With<Rocket>, Without<Wisp>)>,
    wisps_transforms: Query<&Transform, (With<Wisp>, Without<Rocket>)>,
) {
    for (entity, rocket_transform, target, attack_damage) in rockets.iter() {
        let rocket_coords = GridCoords::from_transform(rocket_transform);
        if !rocket_coords.are_in_bounds(wisps_grid.bounds) {
            commands.entity(entity).despawn();
            continue;
        }

        let Some(Ok(wisp_transform)) = target.0.map(|wisp| wisps_transforms.get(wisp)) else { continue; };
        if rocket_transform.translation.xy().distance(wisp_transform.translation.xy()) > ROCKET_HIT_DISTANCE { continue; }

        for direction in ALL_DIRECTIONS.iter().chain(&[(0, 0)]) {
            let blast_zone_coords = rocket_coords.shifted(*direction);
            if !blast_zone_coords.are_in_bounds(wisps_grid.bounds) { continue; }

            commands.spawn(BuilderExplosion(blast_zone_coords));

            let wisps_in_coords = &wisps_grid[blast_zone_coords];
            for wisp in wisps_in_coords {
                if !wisps_transforms.contains(*wisp) { continue; } // May not find wisp if the wisp spawned at the same frame.
                damage_messages.write(DamageMessage {
                    target: *wisp,
                    amount: attack_damage.get(),
                });
            }
        }
        commands.entity(entity).despawn();
    }
}

fn exhaust_blinking_system(
    time: Res<Time>,
    mut exhausts: Query<&mut Sprite, With<RocketExhaust>>,
) {
    let alpha = if time.elapsed_secs() % 1. < 0.85 { 1. } else { 0. };
    for mut sprite in exhausts.iter_mut() {
        sprite.color.set_alpha(alpha);
    }
}
