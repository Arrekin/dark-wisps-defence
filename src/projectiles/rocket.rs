use bevy::sprite::Anchor;

use lib_grid::search::common::ALL_DIRECTIONS;
use lib_grid::grids::wisps::WispsGrid;

use crate::prelude::*;
use crate::projectiles::components::Projectile;
use crate::wisps::components::Wisp;
use crate::effects::explosions::BuilderExplosion;

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
            .add_observer(BuilderRocket::on_add)
            .register_db_loader::<BuilderRocket>(MapLoadingStage::SpawnMapElements)
            .register_db_saver(BuilderRocket::on_game_save);
    }
}

pub const ROCKET_BASE_IMAGE: &str = "projectiles/rocket.png";
pub const ROCKET_EXHAUST_IMAGE: &str = "projectiles/rocket_exhaust.png";

#[derive(Component)]
#[require(AttackDamage, Projectile)]
pub struct Rocket;
#[derive(Component)]
#[require(ZDepth = Z_PROJECTILE_UNDER)]
pub struct RocketExhaust;

// Rocket follows Wisp, and if the wisp no longer exists, looks for another target
#[derive(Component)]
pub struct RocketTarget(pub Entity);

#[derive(Clone, Copy, Debug)]
pub struct RocketSaveData {
    entity: Entity,
}

#[derive(Component, SSS)]
pub struct BuilderRocket {
    world_position: Vec2,
    rotation: Quat,
    target_wisp: Entity,
    damage: AttackDamage,
    save_data: Option<RocketSaveData>,
}
impl Saveable for BuilderRocket {
    fn save(self, tx: &rusqlite::Transaction) -> rusqlite::Result<()> {
        let save_data = self.save_data.expect("BuilderRocket for saving must have save_data");
        let entity_id = save_data.entity.index_u32() as i64;
        let target_wisp_id = self.target_wisp.index_u32() as i64;
        
        // Convert Quat rotation to z-angle
        let (axis, angle) = self.rotation.to_axis_angle();
        let rotation_z = if axis.z > 0.0 { angle } else { -angle };

        tx.register_entity(entity_id)?;
        tx.save_world_position(entity_id, self.world_position)?;
        tx.execute(
            "INSERT OR REPLACE INTO rockets (id, target_wisp_id, rotation_z, damage) VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params![entity_id, target_wisp_id, rotation_z, self.damage.0],
        )?;
        Ok(())
    }
}
impl Loadable for BuilderRocket {
    fn load(ctx: &mut LoadContext) -> rusqlite::Result<LoadResult> {
        let mut stmt = ctx.conn.prepare("SELECT id, target_wisp_id, rotation_z, damage FROM rockets LIMIT ?1 OFFSET ?2")?;
        let mut rows = stmt.query(ctx.pagination.as_params())?;
        
        let mut count = 0;
        while let Some(row) = rows.next()? {
            let old_id: i64 = row.get(0)?;
            let target_wisp_old_id: Option<i64> = row.get(1)?;
            let rotation_z: f32 = row.get(2)?;
            let damage_val: f32 = row.get(3)?;
            let world_position = ctx.conn.get_world_position(old_id)?;
            
            if let Some(new_entity) = ctx.get_new_entity_for_old(old_id) {
                let new_target_wisp = target_wisp_old_id
                    .and_then(|id| ctx.get_new_entity_for_old(id))
                    .unwrap_or(Entity::PLACEHOLDER);
                
                let save_data = RocketSaveData { entity: new_entity };
                ctx.commands.entity(new_entity).insert(BuilderRocket::new_for_saving(
                    world_position,
                    Quat::from_rotation_z(rotation_z),
                    new_target_wisp,
                    AttackDamage(damage_val),
                    save_data
                ));
            }
            count += 1;
        }
        Ok(count.into())
    }
}

impl BuilderRocket {
    pub fn new(world_position: Vec2, rotation: Quat, target_wisp: Entity, damage: AttackDamage) -> Self {
        Self { world_position, rotation, target_wisp, damage, save_data: None }
    }
    pub fn new_for_saving(world_position: Vec2, rotation: Quat, target_wisp: Entity, damage: AttackDamage, save_data: RocketSaveData) -> Self {
        Self { world_position, rotation, target_wisp, damage, save_data: Some(save_data) }
    }

    fn on_game_save(
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

    fn on_add(
        trigger: On<Add, BuilderRocket>,
        mut commands: Commands,
        builders: Query<&BuilderRocket>,
        asset_server: Res<AssetServer>,
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
}


pub fn rocket_move_system(
    mut rockets: Query<(&mut Transform, &mut RocketTarget), With<Rocket>>,
    time: Res<Time>,
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

pub fn rocket_hit_system(
    mut commands: Commands,
    rockets: Query<(Entity, &Transform, &RocketTarget, &AttackDamage), (With<Rocket>, Without<Wisp>)>,
    wisps_grid: Res<WispsGrid>,
    wisps_transforms: Query<&Transform, (With<Wisp>, Without<Rocket>)>,
    mut wisps_health: Query<&mut Health, With<Wisp>>,
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
                let Ok(mut health) = wisps_health.get_mut(*wisp) else { continue }; // May not find wisp if the wisp spawned at the same frame.
                health.decrease(attack_damage.0);
            }
        }
        commands.entity(entity).despawn();
    }
}

pub fn exhaust_blinking_system(
    time: Res<Time>,
    mut query: Query<(&mut Sprite, &RocketExhaust)>,
) {
    for (mut sprite, _) in query.iter_mut() {
        sprite.color.set_alpha(if time.elapsed_secs() % 1. < 0.85 { 1. } else { 0.0 });
    }
}