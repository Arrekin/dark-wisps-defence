use bevy::{
    prelude::*,
    sprite::Anchor,
};

use alteration::modifiers::prelude::AttackDamage;
use game_core::prelude::{DamageMessage, GridCoords, Property, Z_PROJECTILE};
use grids::{
    prelude::WispsGrid,
    search::common::ALL_DIRECTIONS,
};
use persistence::prelude::{AppGameLoadSaveExtension, SaveableBatchCommand};
use states::prelude::{GameState, MapLoadingStage};
use visuals::prelude::BuilderExplosion;
use weaponry::{
    prelude::*,
    rocket::RocketSaveData,
};
use wisps::prelude::Wisp;

/// Plugin for the Rocket projectile
pub struct RocketPlugin;
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
            .register_db_loader::<BuilderRocket>(MapLoadingStage::SpawnMapElements)
            .register_db_saver(on_game_save_collect_rockets);
    }
}

pub(crate) const ROCKET_BASE_IMAGE: &str = "projectiles/rocket.png";
pub(crate) const ROCKET_EXHAUST_IMAGE: &str = "projectiles/rocket_exhaust.png";

fn on_game_save_collect_rockets(
    mut commands: Commands,
    rockets: Query<(Entity, &Transform, &RocketTarget, &AttackDamage), With<Rocket>>,
) {
    if rockets.is_empty() { return; }
    let batch = rockets.iter().map(|(entity, transform, target, damage)| {
         let save_data = RocketSaveData { entity };
         BuilderRocket::new_for_saving(
             transform.translation.xy(),
             transform.rotation,
             target.0,
             damage.clone(),
             save_data
         )
    }).collect::<SaveableBatchCommand<_>>();
    commands.queue(batch);
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
                ..Default::default()
            },
            Transform {
                translation: builder.world_position.extend(Z_PROJECTILE),
                rotation: builder.rotation,
                ..default()
            },
            Rocket,
            RocketTarget(builder.target_wisp),
            builder.damage.clone(),
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

        // Calculate the current forward direction (assuming it's the local y-axis)
        let current_direction = transform.local_x().xy();

        // Move the entity forward (along the local y-axis)
        transform.translation += (current_direction * time.delta_secs() * 400.0).extend(0.0);

        // Calculate the target angle
        let target_angle = direction_vector.y.atan2(direction_vector.x);
        let current_angle = current_direction.y.atan2(current_direction.x);

        // Calculate the shortest rotation to the target angle
        let mut angle_diff = target_angle - current_angle;
        if angle_diff > std::f32::consts::PI {
            angle_diff -= 2.0 * std::f32::consts::PI;
        } else if angle_diff < -std::f32::consts::PI {
            angle_diff += 2.0 * std::f32::consts::PI;
        }

        // Apply the rotation smoothly
        let rotation_speed = 1.5; // radians per second
        let max_rotation_speed = rotation_speed * time.delta_secs();
        let rotation_amount = angle_diff.clamp(-max_rotation_speed, max_rotation_speed);
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
        let rocket_coords = GridCoords::from_transform(&rocket_transform);
        if !rocket_coords.is_in_bounds(wisps_grid.bounds()) {
            commands.entity(entity).despawn();
            continue;
        }

        let Ok(wisp_transform) = wisps_transforms.get(target.0) else { continue };
        if rocket_transform.translation.xy().distance(wisp_transform.translation.xy()) > 6. { continue; }

        let coords = GridCoords::from_transform(&rocket_transform);
        for (dx, dy) in ALL_DIRECTIONS.iter().chain(&[(0, 0)]) {
            let blast_zone_coords = coords.shifted((*dx, *dy));
            if !blast_zone_coords.is_in_bounds(wisps_grid.bounds()) { continue; }

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
    mut query: Query<(&mut Sprite, &RocketExhaust)>,
) {
    for (mut sprite, _) in query.iter_mut() {
        sprite.color.set_alpha(if time.elapsed_secs() % 1. < 0.85 { 1. } else { 0.0 });
    }
}
