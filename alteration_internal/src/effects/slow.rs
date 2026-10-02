use bevy::{platform::collections::HashMap, prelude::*};

use alteration::{
    effects::{prelude::*, slow::SlowEffect},
    modifiers::ModifierType,
};

pub(crate) struct SlowEffectPlugin;
impl Plugin for SlowEffectPlugin {
    fn build(&self, app: &mut App) {
        app
            .add_observer(on_builder_add_spawn_slow_effect);
    }
}

fn on_builder_add_spawn_slow_effect(
    trigger: On<Add, BuilderSlowEffect>,
    mut commands: Commands,
    builders: Query<&BuilderSlowEffect>,
) {
    let entity = trigger.entity;
    let Ok(builder) = builders.get(entity) else { return; };

    let mut entity_commands = commands.entity(entity);
    entity_commands
        .remove::<BuilderSlowEffect>()
        .insert((
            EffectTarget(builder.target_entity),
            ModifierContributions(HashMap::from([(ModifierType::MovementSpeed, -builder.slow_amount)])),
            SlowEffect,
            FieldEffect,
        ));
    if let Some(source_entity) = builder.source_entity {
        entity_commands.insert(EffectSource(source_entity));
    }
}
