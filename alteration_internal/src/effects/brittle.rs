use bevy::{platform::collections::HashMap, prelude::*};

use alteration::{
    effects::{brittle::BrittleEffect, prelude::*},
    modifiers::ModifierType,
};
use game_core::prelude::InsertSome;
use logging::prelude::*;
use persistence::{prelude::*, rusqlite};
use states::MapLoadingStage;

pub(crate) struct BrittleEffectPlugin;
impl Plugin for BrittleEffectPlugin {
    fn build(&self, app: &mut App) {
        app
            .add_observer(on_builder_add_spawn_brittle_effect)
            .add_systems(CollectSave, collect_brittle_effects)
            .register_loader(MapLoadingStage::SpawnEffectInstances, "brittle_effects", load_brittle_effects);
    }
}

#[log_tags(Tag::GameSave)]
fn collect_brittle_effects(
    brittle_effects: Query<(Entity, &EffectTarget, Option<&EffectSource>, &ModifierContributions, Option<&ExpiresAt>), With<BrittleEffect>>,
    mut save: SaveWriter,
) {
    if brittle_effects.is_empty() { return; }
    #[debug_dev("Saving {} brittle effects", rows.len())]
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
                effect_source.map(|source| source.0.index_u32() as i64),
                damage_multiplier,
                expires_at.map(|expires_at| expires_at.0),
            )
        })
        .collect();
    save.submit(move |ctx| {
        for (id, target_id, source_id, damage_multiplier, expires_at) in rows {
            ctx.register_entity(id)?;
            ctx.tx.execute(
                "INSERT OR REPLACE INTO brittle_effects (id, target_id, source_id, damage_multiplier, expires_at) VALUES (?1, ?2, ?3, ?4, ?5)",
                rusqlite::params![id, target_id, source_id, damage_multiplier, expires_at],
            )?;
        }
        Ok(())
    });
}

#[log_tags(Tag::GameLoad)]
fn load_brittle_effects(ctx: &mut LoadContext) -> LoadResult {
    ctx.for_each_entity("SELECT id, target_id, source_id, damage_multiplier, expires_at FROM brittle_effects", |ctx, old_id, entity, row| {
        let old_target_id: i64 = row.get(1)?;
        let old_source_id: Option<i64> = row.get(2)?;
        let damage_multiplier: f32 = row.get(3)?;
        let expires_at: Option<f64> = row.get(4)?;
        let new_target = ctx.entity(old_target_id)?;

        let new_source = ctx.optional_entity(old_source_id)
            .inspect_err(|error| warn_dev!("BrittleEffect old_id={old_id} loads without its source: {error}"))
            .unwrap_or_default();

        let builder = BuilderBrittleEffect::new(new_target, damage_multiplier)
            .with_source(new_source)
            .with_expiry(expires_at.map(ExpiresAt));
        ctx.insert(entity, builder);
        Ok(())
    })
}

fn on_builder_add_spawn_brittle_effect(
    trigger: On<Add, BuilderBrittleEffect>,
    mut commands: Commands,
    builders: Query<&BuilderBrittleEffect>,
) {
    let entity = trigger.entity;
    let Ok(builder) = builders.get(entity) else { return; };

    commands.entity(entity)
        .remove::<BuilderBrittleEffect>()
        .insert((
            EffectTarget(builder.target_entity),
            ModifierContributions(HashMap::from([(ModifierType::IncomingDamageMultiplier, builder.damage_multiplier)])),
            BrittleEffect,
        ))
        .insert_some(builder.source_entity)
        .insert_some(builder.expires_at);
}
