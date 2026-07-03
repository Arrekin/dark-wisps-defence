use bevy::platform::collections::HashMap;
use bevy::prelude::*;

use alteration::effects::brittle::{BrittleEffect, BrittleEffectSaveData};
use alteration::effects::prelude::*;
use alteration::modifiers::ModifierType;
use persistence::prelude::*;
use states::MapLoadingStage;

pub struct BrittleEffectPlugin;
impl Plugin for BrittleEffectPlugin {
    fn build(&self, app: &mut App) {
        app
            .add_observer(build_brittle_effect_on_add)
            .register_db_loader::<BuilderBrittleEffect>(MapLoadingStage::SpawnEffectInstances)
            .register_db_saver(collect_brittle_effects_for_save)
            ;
    }
}

fn collect_brittle_effects_for_save(
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
