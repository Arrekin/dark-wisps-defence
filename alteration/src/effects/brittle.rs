use bevy::prelude::*;

use game_core::prelude::SSS;
use logging::prelude::*;
use persistence::prelude::*;
use persistence::rusqlite;

use super::ExpiresAt;
use super::visual::EffectVisualContribution;

const BRITTLE_BIT: u32 = 1 << 0;
const BRITTLE_SLOT: usize = 0;

/// Marker for the Brittle debuff. Applied to wisps hit by an emitter tower ripple.
/// Causes them to take increased incoming damage for a duration set by the source.
#[derive(Component)]
#[require(EffectVisualContribution = EffectVisualContribution::new(BRITTLE_BIT, BRITTLE_SLOT, Vec4::ZERO))]
pub struct BrittleEffect;

#[derive(Clone, Copy, Debug)]
pub struct BrittleEffectSaveData {
    pub entity: Entity,
}

#[derive(Component, SSS)]
pub struct BuilderBrittleEffect {
    pub target_entity: Entity,
    pub source_entity: Option<Entity>,
    pub damage_multiplier: f32,
    pub expires_at: Option<ExpiresAt>,
    save_data: Option<BrittleEffectSaveData>,
}
impl BuilderBrittleEffect {
    pub fn new(target_entity: Entity, damage_multiplier: f32) -> Self {
        Self { target_entity, source_entity: None, damage_multiplier, expires_at: None, save_data: None }
    }

    pub fn with_source(mut self, source_entity: Entity) -> Self {
        self.source_entity = Some(source_entity);
        self
    }

    pub fn with_expiry(mut self, expires_at: ExpiresAt) -> Self {
        self.expires_at = Some(expires_at);
        self
    }

    pub fn new_for_saving(
        target_entity: Entity,
        source_entity: Option<Entity>,
        damage_multiplier: f32,
        save_data: BrittleEffectSaveData,
    ) -> Self {
        Self { target_entity, source_entity, damage_multiplier, expires_at: None, save_data: Some(save_data) }
    }
}
impl Saveable for BuilderBrittleEffect {
    fn save(self, tx: &rusqlite::Transaction) -> rusqlite::Result<()> {
        let save_data = self.save_data.expect("BuilderBrittleEffect for saving must have save_data");
        let entity_index = save_data.entity.index_u32() as i64;
        let target_index = self.target_entity.index_u32() as i64;
        let source_index = self.source_entity.map(|source| source.index_u32() as i64);

        tx.register_entity(entity_index)?;
        tx.execute(
            "INSERT OR REPLACE INTO brittle_effects (id, target_id, source_id, damage_multiplier, expires_at) VALUES (?1, ?2, ?3, ?4, ?5)",
            rusqlite::params![entity_index, target_index, source_index, self.damage_multiplier, self.expires_at.as_ref().map(|e| e.0)],
        )?;
        Ok(())
    }
}
impl Loadable for BuilderBrittleEffect {
    fn load(ctx: &mut LoadContext) -> rusqlite::Result<LoadResult> {
        let mut stmt = ctx.conn.prepare(
            "SELECT id, target_id, source_id, damage_multiplier, expires_at FROM brittle_effects LIMIT ?1 OFFSET ?2"
        )?;
        let mut rows = stmt.query(ctx.pagination.as_params())?;

        let mut count = 0;
        while let Some(row) = rows.next()? {
            let old_id: i64 = row.get(0)?;
            let old_target_id: i64 = row.get(1)?;
            let old_source_id: Option<i64> = row.get(2)?;
            let damage_multiplier: f32 = row.get(3)?;
            let expires_at: Option<f64> = row.get(4)?;

            let (Some(new_entity), Some(new_target)) = (
                ctx.get_new_entity_for_old(old_id),
                ctx.get_new_entity_for_old(old_target_id),
            ) else {
                Log::warn().dev().tag(Tag::GameLoad).message(format!(
                    "BrittleEffect old_id={old_id} or target old_id={old_target_id} has no mapped entity"
                ));
                continue;
            };

            let new_source = old_source_id.and_then(|old_source_id| {
                let new_source = ctx.get_new_entity_for_old(old_source_id);
                if new_source.is_none() {
                    Log::warn().dev().tag(Tag::GameLoad).message(format!(
                        "BrittleEffect old_id={old_id} source old_id={old_source_id} has no mapped entity"
                    ));
                }
                new_source
            });

            let save_data = BrittleEffectSaveData { entity: new_entity };
            let mut builder = BuilderBrittleEffect::new_for_saving(new_target, new_source, damage_multiplier, save_data);
            if let Some(at) = expires_at {
                builder = builder.with_expiry(ExpiresAt(at));
            }
            ctx.commands.entity(new_entity).insert(builder);
            count += 1;
        }
        Ok(count.into())
    }
}
