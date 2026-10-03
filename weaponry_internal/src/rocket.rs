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
    rockets: Query<(Entity, &Transform, &RocketTarget, &AttackDamage), With<Rocket>>,
    mut save: SaveWriter,
) {
    if rockets.is_empty() { return; }

    #[debug_dev("Saving {} rockets", rows.len())]
    let rows: Vec<(i64, Vec2, Option<i64>, f32, f32)> = rockets
        .iter()
        .map(|(entity, transform, target, damage)| {
            let (axis, angle) = transform.rotation.to_axis_angle();
            let rotation_z = if axis.z > 0.0 { angle } else { -angle };
            (
                entity.index_u32() as i64,
                transform.translation.xy(),
                Some(target.0.index_u32() as i64),
                rotation_z,
                damage.get(),
            )
        })
        .collect();
    save.submit(move |ctx| {
        for (id, position, target_wisp_id, rotation_z, damage) in rows {
            ctx.register_entity(id)?;
            ctx.save_world_position(id, position)?;
            ctx.tx.execute(
                "INSERT OR REPLACE INTO rockets (id, target_wisp_id, rotation_z, damage) VALUES (?1, ?2, ?3, ?4)",
                rusqlite::params![id, target_wisp_id, rotation_z, damage],
            )?;
        }
        Ok(())
    });
}

fn load_rockets(ctx: &mut LoadContext) -> LoadResult {
    ctx.for_each_entity("SELECT id, target_wisp_id, rotation_z, damage FROM rockets", |ctx, old_id, entity, row| {
        let target_wisp_old_id: Option<i64> = row.get(1)?;
        let rotation_z: f32 = row.get(2)?;
        let damage: f32 = row.get(3)?;
        let world_position = ctx.world_position(old_id)?;
        let new_target_wisp = ctx.optional_entity(target_wisp_old_id)
            .unwrap_or_default()
            .unwrap_or(Entity::PLACEHOLDER);

        let builder = BuilderRocket::new(
            world_position,
            Quat::from_rotation_z(rotation_z),
            new_target_wisp,
            AttackDamage::new(damage),
        );
        ctx.insert(entity, builder);
        Ok(())
    })
}

fn on_builder_add_spawn_rocket(
    trigger: On<Add, BuilderRocket>,
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
                rotation: builder.rotation,
                ..default()
            },
            Rocket,
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
    mut rockets: Query<(&mut Transform, &mut RocketTarget), With<Rocket>>,
    wisps: Query<(Entity, &Transform), (With<Wisp>, Without<Rocket>)>,
) {
    let mut wisps_iter = wisps.iter();
    for (mut transform, mut target) in rockets.iter_mut() {
        let target_position = if let Ok((_, wisp_transform)) = wisps.get(target.0) {
            wisp_transform.translation.xy()
        } else {
            wisps_iter.next().map_or(Vec2::ZERO, |(wisp_entity, wisp_transform)| {
                target.0 = wisp_entity;
                wisp_transform.translation.xy()
            })
        };

        // Calculate the direction vector to the target
        let direction_vector = (target_position - transform.translation.xy()).normalize();

        // Calculate the current forward direction (the local x-axis)
        let current_direction = transform.local_x().xy();

        // Move the entity forward (along the local x-axis)
        transform.translation += (current_direction * time.delta_secs() * ROCKET_SPEED).extend(0.0);

        // Calculate the target angle
        let target_angle = direction_vector.y.atan2(direction_vector.x);
        let current_angle = current_direction.y.atan2(current_direction.x);

        // Calculate the shortest rotation to the target angle
        let angle_delta = angle_difference(target_angle, current_angle);

        // Apply the rotation smoothly
        let max_rotation_speed = ROCKET_TURN_SPEED * time.delta_secs();
        let rotation_amount = angle_delta.clamp(-max_rotation_speed, max_rotation_speed);
        transform.rotate(Quat::from_rotation_z(rotation_amount));
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

        let Ok(wisp_transform) = wisps_transforms.get(target.0) else { continue };
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
    for mut sprite in exhausts.iter_mut() {
        sprite.color.set_alpha(if time.elapsed_secs() % 1. < 0.85 { 1. } else { 0.0 });
    }
}
