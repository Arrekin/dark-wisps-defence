use bevy::platform::collections::HashMap;
use bevy::prelude::*;

use alteration::effects::prelude::*;
use alteration::effects::slow::SlowEffect;
use alteration::modifiers::ModifierType;

pub struct SlowEffectPlugin;
impl Plugin for SlowEffectPlugin {
    fn build(&self, app: &mut App) {
        app
            .add_observer(build_slow_effect_on_add)
            ;
    }
}

fn build_slow_effect_on_add(
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
