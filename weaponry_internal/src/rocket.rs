use bevy::{
    prelude::*,
    sprite::Anchor,
};

use alteration::modifiers::prelude::AttackDamage;
use game_core::prelude::{DamageMessage, GridCoords, Property, Z_PROJECTILE};
use grids::{
    search::common::ALL_DIRECTIONS,
    wisps::WispsGrid,
};
use logging::prelude::*;
use persistence::{
    prelude::{AppGameLoadSaveExtension, CollectSave, GameDbHelpers, LoadContext, SaveWriter},
    rusqlite,
};
use states::prelude::{GameState, MapLoadingStage};
use visuals::prelude::BuilderExplosion;
use weaponry::prelude::*;
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
            .add_systems(CollectSave, collect_rockets)
            .register_loader(MapLoadingStage::SpawnMapElements, "rockets", load_rockets)
            ;
    }
}

pub(crate) const ROCKET_BASE_IMAGE: &str = "projectiles/rocket.png";
pub(crate) const ROCKET_EXHAUST_IMAGE: &str = "projectiles/rocket_exhaust.png";

fn collect_rockets(
    rockets: Query<(Entity, &Transform, &RocketTarget, &AttackDamage), With<Rocket>>,
    mut save: SaveWriter,
) {
    if rockets.is_empty() { return; }
    let rows: Vec<(i64, f32, f32, Option<i64>, f32, f32)> = rockets
        .iter()
        .map(|(entity, transform, target, damage)| {
            let (axis, angle) = transform.rotation.to_axis_angle();
            let rotation_z = if axis.z > 0.0 { angle } else { -angle };
            (
                entity.index_u32() as i64,
                transform.translation.x,
                transform.translation.y,
                Some(target.0.index_u32() as i64),
                rotation_z,
                damage.get(),
            )
        })
        .collect();
    save.submit(move |tx| {
        for (id, pos_x, pos_y, target_wisp_id, rotation_z, damage) in rows {
            tx.register_entity(id)?;
            tx.save_world_position(id, Vec2::new(pos_x, pos_y))?;
            tx.execute(
                "INSERT OR REPLACE INTO rockets (id, target_wisp_id, rotation_z, damage) VALUES (?1, ?2, ?3, ?4)",
                rusqlite::params![id, target_wisp_id, rotation_z, damage],
            )?;
        }
        Ok(())
    });
}

fn load_rockets(ctx: &mut LoadContext) -> rusqlite::Result<()> {
    let mut stmt = ctx.conn.prepare("SELECT id, target_wisp_id, rotation_z, damage FROM rockets")?;
    let mut rows = stmt.query([])?;
    while let Some(row) = rows.next()? {
        let old_id: i64 = row.get(0)?;
        let target_wisp_old_id: Option<i64> = row.get(1)?;
        let rotation_z: f32 = row.get(2)?;
        let damage_val: f32 = row.get(3)?;
        let world_position = ctx.conn.get_world_position(old_id)?;

        let Some(entity) = ctx.entity(old_id) else {
            Log::warn().dev().tag(Tag::GameLoad).message(format!(
                "rockets: unmapped id for row {old_id}"
            ));
            continue;
        };
        let new_target_wisp = target_wisp_old_id
            .and_then(|id| ctx.entity(id))
            .unwrap_or(Entity::PLACEHOLDER);

        let builder = BuilderRocket::new(
            world_position,
            Quat::from_rotation_z(rotation_z),
            new_target_wisp,
            AttackDamage::new(damage_val),
        );
        ctx.insert(entity, builder);
    }
    Ok(())
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
