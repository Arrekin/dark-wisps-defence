use bevy::{platform::collections::HashMap, prelude::*};

use alteration::{
    effects::{prelude::*, slow::SlowEffect},
    modifiers::ModifierType,
};
use game_core::prelude::OptionalCommands;

pub(crate) struct SlowEffectPlugin;
impl Plugin for SlowEffectPlugin {
    fn build(&self, app: &mut App) {
        app
            .add_observer(on_builder_add_spawn_slow_effect);
    }
}

fn on_builder_add_spawn_slow_effect(
    trigger: On<Add<BuilderSlowEffect>>,
    mut commands: Commands,
    builders: Query<&BuilderSlowEffect>,
) {
    let entity = trigger.entity;
    let Ok(builder) = builders.get(entity) else { return; };

    commands.entity(entity)
        .remove::<BuilderSlowEffect>()
        .insert((
            EffectTarget(builder.target_entity),
            ModifierContributions(HashMap::from([(ModifierType::MovementSpeed, -builder.slow_amount)])),
            SlowEffect,
            FieldEffect,
        ))
        .insert_some(builder.source_entity);
}
