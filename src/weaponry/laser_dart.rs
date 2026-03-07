use lib_grid::grids::wisps::WispsGrid;

use crate::prelude::*;
use crate::weaponry::components::Projectile;
use crate::wisps::components::Wisp;

pub struct LaserDartPlugin;
impl Plugin for LaserDartPlugin {
    fn build(&self, app: &mut App) {
        app
            .add_systems(Update, (
                (
                    laser_dart_move_system,
                    laser_dart_hit_system,
                ).run_if(in_state(GameState::Running)),
            ))
            .add_observer(BuilderLaserDart::on_add)
            .register_db_loader::<BuilderLaserDart>(MapLoadingStage::SpawnMapElements)
            .register_db_saver(BuilderLaserDart::on_game_save);
    }
}

#[derive(Component)]
pub struct LaserDart;

// LaserDart follows Wisp, and if the wisp no longer exists, follows the target vector
#[derive(Component, Default)]
#[require(AttackDamage, Projectile)]
pub struct LaserDartTarget {
    pub target_wisp: Option<Entity>,
    pub target_vector: Vec2,
}

#[derive(Clone, Copy, Debug)]
pub struct LaserDartSaveData {
    pub entity: Entity,
}

#[derive(Component, SSS)]
pub struct BuilderLaserDart {
    pub world_position: Vec2,
    pub target_wisp: Option<Entity>,
    pub target_vector: Vec2,
    pub damage: AttackDamage,
    pub save_data: Option<LaserDartSaveData>,
}
impl Saveable for BuilderLaserDart {
    fn save(self, tx: &rusqlite::Transaction) -> rusqlite::Result<()> {
        let save_data = self.save_data.expect("BuilderLaserDart for saving must have save_data");
        let entity_id = save_data.entity.index_u32() as i64;
        let target_wisp_id = self.target_wisp.map(|e| e.index_u32() as i64);

        tx.register_entity(entity_id)?;
        tx.save_world_position(entity_id, self.world_position)?;
        tx.execute(
            "INSERT OR REPLACE INTO laser_darts (id, target_wisp_id, vector_x, vector_y, damage) VALUES (?1, ?2, ?3, ?4, ?5)",
            rusqlite::params![entity_id, target_wisp_id, self.target_vector.x, self.target_vector.y, self.damage.get()],
        )?;
        Ok(())
    }
}
impl Loadable for BuilderLaserDart {
    fn load(ctx: &mut LoadContext) -> rusqlite::Result<LoadResult> {
        let mut stmt = ctx.conn.prepare("SELECT id, target_wisp_id, vector_x, vector_y, damage FROM laser_darts LIMIT ?1 OFFSET ?2")?;
        let mut rows = stmt.query(ctx.pagination.as_params())?;
        
        let mut count = 0;
        while let Some(row) = rows.next()? {
            let old_id: i64 = row.get(0)?;
            let target_wisp_old_id: Option<i64> = row.get(1)?;
            let vector_x: f32 = row.get(2)?;
            let vector_y: f32 = row.get(3)?;
            let damage_val: f32 = row.get(4)?;
            let world_position = ctx.conn.get_world_position(old_id)?;
            
            let Some(new_entity) = ctx.get_new_entity_for_old(old_id) else { continue; };
            let new_target_wisp = target_wisp_old_id.and_then(|id| ctx.get_new_entity_for_old(id));
            
            let save_data = LaserDartSaveData { entity: new_entity };
            ctx.commands.entity(new_entity).insert(BuilderLaserDart::new_for_saving(
                world_position,
                new_target_wisp,
                Vec2::new(vector_x, vector_y),
                AttackDamage::new(damage_val),
                save_data
            ));
            count += 1;
        }
        Ok(count.into())
    }
}

impl BuilderLaserDart {
    pub fn new(world_position: Vec2, target_wisp: Entity, target_vector: Vec2, damage: AttackDamage) -> Self {
        Self { world_position, target_wisp: Some(target_wisp), target_vector, damage, save_data: None }
    }
    pub fn new_for_saving(world_position: Vec2, target_wisp: Option<Entity>, target_vector: Vec2, damage: AttackDamage, save_data: LaserDartSaveData) -> Self {
        Self { world_position, target_wisp, target_vector, damage, save_data: Some(save_data) }
    }

    fn on_game_save(
        mut commands: Commands,
        laser_darts: Query<(Entity, &Transform, &LaserDartTarget, &AttackDamage), With<LaserDart>>,
    ) {
        if laser_darts.is_empty() { return; }
        let batch = laser_darts.iter().map(|(entity, transform, target, damage)| {
             let save_data = LaserDartSaveData { entity };
             BuilderLaserDart::new_for_saving(
                 transform.translation.xy(),
                 target.target_wisp,
                 target.target_vector,
                 damage.clone(),
                 save_data
             )
        }).collect::<SaveableBatchCommand<_>>();
        commands.queue(batch);
    }

    fn on_add(
        trigger: On<Add, BuilderLaserDart>,
        mut commands: Commands,
        builders: Query<&BuilderLaserDart>,
    ) {
        let entity = trigger.entity;
        let Ok(builder) = builders.get(entity) else { return; };
        
        commands.entity(entity)
            .remove::<BuilderLaserDart>()
            .insert((
                Sprite {
                    color: Color::srgb(1.0, 0.0, 0.0),
                    custom_size: Some(Vec2::new(14.0, 2.0)),
                    ..Default::default()
                },
                Transform {
                    translation: builder.world_position.extend(Z_PROJECTILE),
                    rotation: Quat::from_rotation_z(builder.target_vector.y.atan2(builder.target_vector.x)),
                    ..Default::default()
                },
                LaserDart,
                LaserDartTarget{ target_wisp: builder.target_wisp, target_vector: builder.target_vector },
                builder.damage.clone(),
            ));
    }
}

pub fn laser_dart_move_system(
    mut laser_darts: Query<(&mut Transform, &mut LaserDartTarget), With<LaserDart>>,
    wisps: Query<&Transform, (With<Wisp>, Without<LaserDart>)>,
    time: Res<Time>,
) {
    for (mut transform, mut target) in laser_darts.iter_mut() {
        // If the target wisp still exists - follow it by updating the target vector
        if let Some(target_wisp) = target.target_wisp {
            if let Ok(wisp_transform) = wisps.get(target_wisp) {
                target.target_vector = (wisp_transform.translation.xy() - transform.translation.xy()).normalize();
            } else {
                target.target_wisp = None;
            }
        }
        transform.translation += target.target_vector.extend(0.) * time.delta_secs() * 600.;
    }
}

pub fn laser_dart_hit_system(
    mut commands: Commands,
    mut damage_messages: MessageWriter<DamageMessage>,
    laser_darts: Query<(Entity, &Transform, &AttackDamage), With<LaserDart>>,
    wisps_grid: Res<WispsGrid>,
    wisps: Query<&Transform, With<Wisp>>,
) {
    for (entity, laser_dart_transform, damage) in laser_darts.iter() {
        let coords = GridCoords::from_transform(&laser_dart_transform);
        if !coords.is_in_bounds(wisps_grid.bounds()) {
            commands.entity(entity).despawn();
            continue;
        }
        let wisps_in_coords = &wisps_grid[coords];
        for wisp in wisps_in_coords {
            let Ok(wisp_transform) = wisps.get(*wisp) else { continue }; // May not find wisp if the wisp spawned at the same frame.
            if laser_dart_transform.translation.xy().distance(wisp_transform.translation.xy()) < 8. {
                damage_messages.write(DamageMessage {
                    target: *wisp,
                    amount: damage.get(),
                });
                commands.entity(entity).despawn();
                break;
            }
        }
    }
}