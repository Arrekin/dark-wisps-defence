use bevy::platform::collections::HashMap;
use bevy::prelude::*;

use alteration::effects::brittle::BrittleEffect;
use alteration::effects::prelude::*;
use alteration::modifiers::ModifierType;
use logging::prelude::*;
use persistence::{
    prelude::{AppGameLoadSaveExtension, CollectSave, GameDbHelpers, LoadContext, SaveWriter},
    rusqlite,
};
use states::MapLoadingStage;

pub struct BrittleEffectPlugin;
impl Plugin for BrittleEffectPlugin {
    fn build(&self, app: &mut App) {
        app
            .add_observer(build_brittle_effect_on_add)
            .add_systems(CollectSave, collect_brittle_effects_for_save)
            .register_loader(MapLoadingStage::SpawnEffectInstances, "brittle_effects", load_brittle_effects)
            ;
    }
}

fn collect_brittle_effects_for_save(
    brittle_effects: Query<(Entity, &EffectTarget, Option<&EffectSource>, &ModifierContributions, Option<&ExpiresAt>), With<BrittleEffect>>,
    mut save: SaveWriter,
) {
    if brittle_effects.is_empty() { return; }
    let rows: Vec<(i64, i64, Option<i64>, f32, Option<f64>)> = brittle_effects
        .iter()
        .map(|(entity, effect_target, effect_source, contributions, expires_at)| {
            let damage_multiplier = contributions.0
                .get(&ModifierType::IncomingDamageMultiplier)
                .copied()
                .unwrap_or(1.0);
            (
                entity.index_u32() as i64,
                effect_target.0.index_u32() as i64,
                effect_source.map(|s| s.0.index_u32() as i64),
                damage_multiplier,
                expires_at.map(|e| e.0),
            )
        })
        .collect();
    save.submit(move |tx| {
        for (id, target_id, source_id, damage_multiplier, expires_at) in rows {
            tx.register_entity(id)?;
            tx.execute(
                "INSERT OR REPLACE INTO brittle_effects (id, target_id, source_id, damage_multiplier, expires_at) VALUES (?1, ?2, ?3, ?4, ?5)",
                rusqlite::params![id, target_id, source_id, damage_multiplier, expires_at],
            )?;
        }
        Ok(())
    });
}

fn load_brittle_effects(ctx: &mut LoadContext) -> rusqlite::Result<()> {
    let mut stmt = ctx.conn.prepare(
        "SELECT id, target_id, source_id, damage_multiplier, expires_at FROM brittle_effects"
    )?;
    let mut rows = stmt.query([])?;
    while let Some(row) = rows.next()? {
        let old_id: i64 = row.get(0)?;
        let old_target_id: i64 = row.get(1)?;
        let old_source_id: Option<i64> = row.get(2)?;
        let damage_multiplier: f32 = row.get(3)?;
        let expires_at: Option<f64> = row.get(4)?;

        let (Some(entity), Some(new_target)) = (
            ctx.entity(old_id),
            ctx.entity(old_target_id),
        ) else {
            Log::warn().dev().tag(Tag::GameLoad).message(format!(
                "BrittleEffect old_id={old_id} or target old_id={old_target_id} has no mapped entity"
            ));
            continue;
        };

        let new_source = old_source_id.and_then(|old_source_id| {
            let new_source = ctx.entity(old_source_id);
            if new_source.is_none() {
                Log::warn().dev().tag(Tag::GameLoad).message(format!(
                    "BrittleEffect old_id={old_id} source old_id={old_source_id} has no mapped entity"
                ));
            }
            new_source
        });

        let mut builder = BuilderBrittleEffect::new(new_target, damage_multiplier);
        if let Some(source) = new_source {
            builder = builder.with_source(source);
        }
        if let Some(at) = expires_at {
            builder = builder.with_expiry(ExpiresAt(at));
        }
        ctx.insert(entity, builder);
    }
    Ok(())
}

fn build_brittle_effect_on_add(
    trigger: On<Add, BuilderBrittleEffect>,
    mut commands: Commands,
    builders: Query<&BuilderBrittleEffect>,
) {
    let entity = trigger.entity;
    let Ok(builder) = builders.get(entity) else { return; };

    let mut entity_commands = commands.entity(entity);
    entity_commands
        .remove::<BuilderBrittleEffect>()
        .insert((
            EffectTarget(builder.target_entity),
            ModifierContributions(HashMap::from([(ModifierType::IncomingDamageMultiplier, builder.damage_multiplier)])),
            BrittleEffect,
        ));
    if let Some(source_entity) = builder.source_entity {
        entity_commands.insert(EffectSource(source_entity));
    }
    if let Some(expires_at) = builder.expires_at {
        entity_commands.insert(expires_at);
    }
}
