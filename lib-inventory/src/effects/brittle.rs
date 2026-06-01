use crate::lib_prelude::*;

/// Marker for the Brittle debuff. Applied to wisps hit by an emitter tower ripple.
/// Causes them to take increased incoming damage for a duration set by the source.
#[derive(Component)]
#[require(EffectVisualContribution = EffectVisualContribution::new(BRITTLE_BIT, BRITTLE_SLOT, Vec4::ZERO))]
pub struct BrittleEffect;

pub struct BrittleEffectPlugin;
impl Plugin for BrittleEffectPlugin {
    fn build(&self, app: &mut App) {
        app
            .add_observer(BuilderBrittleEffect::on_add)
            .register_db_loader::<BuilderBrittleEffect>(MapLoadingStage::SpawnEffectInstances)
            .register_db_saver(BuilderBrittleEffect::on_game_save)
            ;
    }
}

#[derive(Clone, Copy, Debug)]
struct BrittleEffectSaveData {
    entity: Entity,
}

#[derive(Component, SSS)]
pub struct BuilderBrittleEffect {
    target_entity: Entity,
    source_entity: Option<Entity>,
    damage_multiplier: f32,
    expires_at: Option<ExpiresAt>,
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

    fn new_for_saving(
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
impl BuilderBrittleEffect {
    fn on_game_save(
        mut commands: Commands,
        brittle_effects: Query<(Entity, &EffectTarget, Option<&EffectSource>, &ModifierContributions, Option<&ExpiresAt>), With<BrittleEffect>>,
    ) {
        if brittle_effects.is_empty() { return; }
        let batch = brittle_effects.iter().map(|(entity, effect_target, effect_source, contributions, expires_at)| {
            let damage_multiplier = contributions.0
                .get(&ModifierType::IncomingDamageMultiplier)
                .copied()
                .unwrap_or(1.0);
            let save_data = BrittleEffectSaveData { entity };
            let mut builder = BuilderBrittleEffect::new_for_saving(
                effect_target.0,
                effect_source.map(|source| source.0),
                damage_multiplier,
                save_data,
            );
            if let Some(expires_at) = expires_at {
                builder = builder.with_expiry(*expires_at);
            }
            builder
        }).collect::<SaveableBatchCommand<_>>();
        commands.queue(batch);
    }

    fn on_add(
        trigger: On<Add, BuilderBrittleEffect>,
        mut commands: Commands,
        builders: Query<&BuilderBrittleEffect>,
    ) {
        let entity = trigger.entity;
        let Ok(builder) = builders.get(entity) else { return; };
        let target_entity = builder.target_entity;
        let source_entity = builder.source_entity;
        let damage_multiplier = builder.damage_multiplier;
        let expires_at = builder.expires_at;

        let mut entity_commands = commands.entity(entity);
        entity_commands
            .remove::<BuilderBrittleEffect>()
            .insert((
                EffectTarget(target_entity),
                ModifierContributions(HashMap::from([(ModifierType::IncomingDamageMultiplier, damage_multiplier)])),
                BrittleEffect,
            ));
        if let Some(source_entity) = source_entity {
            entity_commands.insert(EffectSource(source_entity));
        }
        if let Some(expires_at) = expires_at {
            entity_commands.insert(expires_at);
        }
    }
}
