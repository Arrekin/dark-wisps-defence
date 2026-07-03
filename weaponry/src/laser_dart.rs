use bevy::prelude::*;

use alteration::modifiers::prelude::AttackDamage;
use game_core::prelude::{Property, SSS};
use persistence::{
    prelude::{GameDbHelpers, LoadContext, LoadResult, Loadable, Saveable},
    rusqlite,
};

use super::components::Projectile;

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
}
