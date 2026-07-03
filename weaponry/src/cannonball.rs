use bevy::prelude::*;

use alteration::modifiers::prelude::AttackDamage;
use game_core::prelude::{Property, SSS};
use persistence::{
    prelude::{GameDbHelpers, LoadContext, LoadResult, Loadable, Saveable},
    rusqlite,
};

use super::components::Projectile;

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
            rusqlite::params![entity_id, self.target_position.x, self.target_position.y, self.damage.get(), save_data.initial_distance],
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
                AttackDamage::new(damage_val),
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
}
