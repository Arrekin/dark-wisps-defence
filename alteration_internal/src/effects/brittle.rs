use bevy::{platform::collections::HashMap, prelude::*};

use alteration::{
    effects::{brittle::BrittleEffect, prelude::*},
    modifiers::ModifierType,
};
use game_core::prelude::OptionalCommands;
use logging::prelude::*;
use persistence::{prelude::*, rusqlite};
use session::TimeBound;
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
    brittle_effects: Query<(Entity, &EffectTarget, Option<&EffectSource>, &ModifierContributions, Option<&TimeBound>), With<BrittleEffect>>,
    mut save: SaveWriter,
) {
    if brittle_effects.is_empty() { return; }
    #[debug_dev("Saving {} brittle effects", rows.len())]
    let rows: Vec<(u32, u32, Option<u32>, f32, Option<TimeBound>)> = brittle_effects
        .iter()
        .map(|(entity, effect_target, effect_source, contributions, time_bound)| {
            let damage_multiplier = contributions.0
                .get(&ModifierType::IncomingDamageMultiplier)
                .copied()
                .unwrap_or(1.0);
            (
                entity.index_u32(),
                effect_target.0.index_u32(),
                effect_source.map(|source| source.0.index_u32()),
                damage_multiplier,
                time_bound.copied(),
            )
        })
        .collect();
    save.submit(move |ctx| {
        for (id, target_id, source_id, damage_multiplier, time_bound) in rows {
            ctx.register_entity(id)?;
            ctx.tx.execute(
                "INSERT OR REPLACE INTO brittle_effects (id, target_id, source_id, damage_multiplier) VALUES (?1, ?2, ?3, ?4)",
                rusqlite::params![id, target_id, source_id, damage_multiplier],
            )?;
            if let Some(time_bound) = time_bound {
                ctx.save_time_bound(id, time_bound)?;
            }
        }
        Ok(())
    });
}

#[log_tags(Tag::GameLoad)]
fn load_brittle_effects(ctx: &mut LoadContext) -> LoadResult {
    ctx.for_each_entity("SELECT id, target_id, source_id, damage_multiplier FROM brittle_effects", |ctx, old_id, entity, row| {
        let old_target_id: u32 = row.get(1)?;
        let old_source_id: Option<u32> = row.get(2)?;
        let damage_multiplier: f32 = row.get(3)?;
        let time_bound = ctx.time_bound(old_id)?;
        let new_target = ctx.entity(old_target_id)?;

        let new_source = ctx.optional_entity(old_source_id)
            .inspect_err(|error| warn_dev!("BrittleEffect old_id={old_id} loads without its source: {error}"))
            .unwrap_or_default();

        let builder = BuilderBrittleEffect::new(new_target, damage_multiplier)
            .with_source(new_source)
            .with_time_bound(time_bound);
        ctx.insert(entity, builder);
        Ok(())
    })
}

fn on_builder_add_spawn_brittle_effect(
    trigger: On<Add<BuilderBrittleEffect>>,
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
        .insert_some(builder.time_bound);
}
