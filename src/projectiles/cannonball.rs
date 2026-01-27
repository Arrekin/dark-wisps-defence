use std::f32::consts::PI;

use lib_grid::search::common::ALL_DIRECTIONS;
use lib_grid::grids::wisps::WispsGrid;

use crate::prelude::*;
use crate::effects::explosions::BuilderExplosion;
use crate::projectiles::components::Projectile;
use crate::wisps::components::Wisp;

pub struct CannonballPlugin;
impl Plugin for CannonballPlugin {
    fn build(&self, app: &mut App) {
        app
            .add_systems(Update, (
                (
                    cannonball_move_system,
                    cannonball_hit_system,
                ).run_if(in_state(GameState::Running)),
            ))
            .add_observer(BuilderCannonball::on_add)
            .register_db_loader::<BuilderCannonball>(MapLoadingStage::SpawnMapElements)
            .register_db_saver(BuilderCannonball::on_game_save);
    }
}

pub const CANNONBALL_BASE_IMAGE: &str = "projectiles/cannonball.png";

#[derive(Component)]
#[require(AttackDamage, Projectile)]
pub struct Cannonball;

// Cannonball follows Wisp, and if the wisp no longer exists, follows to the target position
#[derive(Component, Default)]
pub struct CannonballTarget{
    pub initial_distance: f32,
    pub target_position: Vec2,
}

#[derive(Clone, Copy, Debug)]
pub struct CannonballSaveData {
    pub entity: Entity,
    pub initial_distance: f32,
}

#[derive(Component, SSS)]
pub struct BuilderCannonball {
    pub world_position: Vec2,
    pub target_position: Vec2,
    pub damage: AttackDamage,
    pub save_data: Option<CannonballSaveData>,
}
impl Saveable for BuilderCannonball {
    fn save(self, tx: &rusqlite::Transaction) -> rusqlite::Result<()> {
        let save_data = self.save_data.expect("BuilderCannonball for saving must have save_data");
        let entity_id = save_data.entity.index_u32() as i64;
        
        tx.register_entity(entity_id)?;
        tx.save_world_position(entity_id, self.world_position)?;
        tx.execute(
            "INSERT OR REPLACE INTO cannonballs (id, target_x, target_y, damage, initial_distance) VALUES (?1, ?2, ?3, ?4, ?5)",
            rusqlite::params![entity_id, self.target_position.x, self.target_position.y, self.damage.0, save_data.initial_distance],
        )?;
        Ok(())
    }
}
impl Loadable for BuilderCannonball {
    fn load(ctx: &mut LoadContext) -> rusqlite::Result<LoadResult> {
        let mut stmt = ctx.conn.prepare("SELECT id, target_x, target_y, damage, initial_distance FROM cannonballs LIMIT ?1 OFFSET ?2")?;
        let mut rows = stmt.query(ctx.pagination.as_params())?;
        
        let mut count = 0;
        while let Some(row) = rows.next()? {
            let old_id: i64 = row.get(0)?;
            let target_x: f32 = row.get(1)?;
            let target_y: f32 = row.get(2)?;
            let damage_val: f32 = row.get(3)?;
            let initial_distance: f32 = row.get(4)?;
            let world_position = ctx.conn.get_world_position(old_id)?;
            
            let Some(new_entity) = ctx.get_new_entity_for_old(old_id) else { continue; };
            let save_data = CannonballSaveData { entity: new_entity, initial_distance };
            ctx.commands.entity(new_entity).insert(BuilderCannonball::new_for_saving(
                world_position,
                Vec2::new(target_x, target_y),
                AttackDamage(damage_val),
                save_data
            ));
            count += 1;
        }
        Ok(count.into())
    }
}

impl BuilderCannonball {
    pub fn new(world_position: Vec2, target_position: Vec2, damage: AttackDamage) -> Self {
        Self { world_position, target_position, damage, save_data: None }
    }
    pub fn new_for_saving(world_position: Vec2, target_position: Vec2, damage: AttackDamage, save_data: CannonballSaveData) -> Self {
        Self { world_position, target_position, damage, save_data: Some(save_data) }
    }

    fn on_game_save(
        mut commands: Commands,
        cannonballs: Query<(Entity, &Transform, &CannonballTarget, &AttackDamage), With<Cannonball>>,
    ) {
        if cannonballs.is_empty() { return; }
        let batch = cannonballs.iter().map(|(entity, transform, target, damage)| {
             let save_data = CannonballSaveData {
                 entity,
                 initial_distance: target.initial_distance,
             };
             BuilderCannonball::new_for_saving(
                 transform.translation.xy(),
                 target.target_position,
                 damage.clone(),
                 save_data
             )
        }).collect::<SaveableBatchCommand<_>>();
        commands.queue(batch);
    }

    fn on_add(
        trigger: On<Add, BuilderCannonball>,
        mut commands: Commands,
        builders: Query<&BuilderCannonball>,
        asset_server: Res<AssetServer>,
    ) {
        let entity = trigger.entity;
        let Ok(builder) = builders.get(entity) else { return; };
        
        let initial_distance = if let Some(save_data) = &builder.save_data {
            save_data.initial_distance
        } else {
            builder.world_position.distance(builder.target_position)
        };
        
        commands.entity(entity)
            .remove::<BuilderCannonball>()
            .insert((
                Sprite {
                    image: asset_server.load(CANNONBALL_BASE_IMAGE),
                    custom_size: Some(Vec2::new(CELL_SIZE / 2., CELL_SIZE / 2.)),
                    ..default()
                },
                Transform::from_translation(builder.world_position.extend(Z_PROJECTILE)),
                Cannonball,
                CannonballTarget {
                    initial_distance,
                    target_position: builder.target_position,
                },
                builder.damage.clone(),
            ));
    }
}

pub fn cannonball_move_system(
    mut cannonballs: Query<(&mut Transform, &CannonballTarget), With<Cannonball>>,
    time: Res<Time>,
) {
    for (mut transform, target) in cannonballs.iter_mut() {
        let direction_vector = (target.target_position - transform.translation.xy()).normalize();
        let move_distance = direction_vector * time.delta_secs() * 400.;

        let remaining_distance = (transform.translation.xy() + move_distance).distance(target.target_position);

        // Calculate the progress as a value between 0 and 1
        let progress = 1. - remaining_distance / target.initial_distance;

        // Determine the scaling factor based on progress, applying a sine function for non-linearity
        let scale_factor = if progress <= 0.5 {
            1.0 + (PI * progress).sin()  // Non-linear scale up in the first half
        } else {
            (PI * (1.0 - progress)).sin() + 1.0  // Non-linear scale down in the second half
        };
        transform.scale = Vec3::splat(scale_factor);

        // Move the cannonball
        transform.translation += move_distance.extend(0.);
    }
}

pub fn cannonball_hit_system(
    mut commands: Commands,
    cannonballs: Query<(Entity, &Transform, &CannonballTarget, &AttackDamage), With<Cannonball>>,
    wisps_grid: Res<WispsGrid>,
    mut wisps: Query<&mut Health, With<Wisp>>,
) {
    for (entity, cannonball_transform, target, attack_damage) in cannonballs.iter() {
        if cannonball_transform.translation.xy().distance(target.target_position) > 4. { continue; } // TODO: 1. and 2. are causing cannonballs jitters at landing. Investigate.

        let coords = GridCoords::from_transform(&cannonball_transform);
        for (dx, dy) in ALL_DIRECTIONS.iter().chain(&[(0, 0)]) {
            let blast_zone_coords = coords.shifted((*dx, *dy));
            if !blast_zone_coords.is_in_bounds(wisps_grid.bounds()) { continue; }

            commands.spawn(BuilderExplosion(blast_zone_coords));

            let wisps_in_coords = &wisps_grid[blast_zone_coords];
            for wisp in wisps_in_coords {
                let Ok(mut health) = wisps.get_mut(*wisp) else { continue }; // May not find wisp if the wisp spawned at the same frame.
                health.decrease(attack_damage.0);
            }
        }
        commands.entity(entity).despawn();
    }
}