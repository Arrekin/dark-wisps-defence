use bevy::prelude::*;

use game_core::prelude::{MapBound, SSS};
use persistence::{
    prelude::{GameDbHelpers, LoadContext, LoadResult, Loadable, Saveable},
    rusqlite,
};

#[derive(Clone, Copy, Debug)]
pub struct RippleSaveData {
    pub entity: Entity,
    pub current_radius: f32,
}

#[derive(Component, SSS)]
pub struct BuilderRipple {
    pub world_position: Vec2,
    pub radius: f32, // in world size
    pub save_data: Option<RippleSaveData>,
}
impl Saveable for BuilderRipple {
    fn save(self, tx: &rusqlite::Transaction) -> rusqlite::Result<()> {
        let save_data = self.save_data.expect("BuilderRipple for saving must have save_data");
        let entity_id = save_data.entity.index_u32() as i64;

        tx.register_entity(entity_id)?;
        tx.save_world_position(entity_id, self.world_position)?;
        tx.execute(
            "INSERT OR REPLACE INTO ripples (id, max_radius, current_radius) VALUES (?1, ?2, ?3)",
            rusqlite::params![entity_id, self.radius, save_data.current_radius],
        )?;
        Ok(())
    }
}
impl Loadable for BuilderRipple {
    fn load(ctx: &mut LoadContext) -> rusqlite::Result<LoadResult> {
        let mut stmt = ctx.conn.prepare("SELECT id, max_radius, current_radius FROM ripples LIMIT ?1 OFFSET ?2")?;
        let mut rows = stmt.query(ctx.pagination.as_params())?;

        let mut count = 0;
        while let Some(row) = rows.next()? {
            let old_id: i64 = row.get(0)?;
            let max_radius: f32 = row.get(1)?;
            let current_radius: f32 = row.get(2)?;
            let world_position = ctx.conn.get_world_position(old_id)?;

            if let Some(new_entity) = ctx.get_new_entity_for_old(old_id) {
                let save_data = RippleSaveData { entity: new_entity, current_radius };
                ctx.commands.entity(new_entity).insert(BuilderRipple::new_for_saving(
                    world_position,
                    max_radius,
                    save_data
                ));
            }
            count += 1;
        }
        Ok(count.into())
    }
}

impl BuilderRipple {
    pub fn new(world_position: Vec2, radius: f32) -> Self {
        Self { world_position, radius, save_data: None }
    }
    pub fn new_for_saving(world_position: Vec2, radius: f32, save_data: RippleSaveData) -> Self {
        Self { world_position, radius, save_data: Some(save_data) }
    }
}

#[derive(Component)]
#[require(MapBound)]
pub struct Ripple {
    pub max_radius: f32,
    pub current_radius: f32,
}
impl Ripple {
    /// Current radius as a fraction of the full diameter, range 0..0.5.
    /// Matches the normalised radius the shader uses internally.
    pub fn normalized_radius(&self) -> f32 {
        self.current_radius / (self.max_radius * 2.0)
    }

    pub fn max_radius(&self) -> f32 {
        self.max_radius
    }
}
