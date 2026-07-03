use bevy::prelude::*;

use alteration::modifiers::prelude::AttackDamage;
use game_core::prelude::{Property, SSS, Z_PROJECTILE_UNDER};
use persistence::{
    prelude::{GameDbHelpers, LoadContext, LoadResult, Loadable, Saveable},
    rusqlite,
};

use super::components::Projectile;

#[derive(Component)]
#[require(AttackDamage, Projectile)]
pub struct Rocket;
#[derive(Component)]
#[require(game_core::prelude::ZDepth = Z_PROJECTILE_UNDER)]
pub struct RocketExhaust;

// Rocket follows Wisp, and if the wisp no longer exists, looks for another target
#[derive(Component)]
pub struct RocketTarget(pub Entity);

#[derive(Clone, Copy, Debug)]
pub struct RocketSaveData {
    pub entity: Entity,
}

#[derive(Component, SSS)]
pub struct BuilderRocket {
    pub world_position: Vec2,
    pub rotation: Quat,
    pub target_wisp: Entity,
    pub damage: AttackDamage,
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
            rusqlite::params![entity_id, target_wisp_id, rotation_z, self.damage.get()],
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
                    AttackDamage::new(damage_val),
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
}
