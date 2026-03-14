use crate::lib_prelude::*;

/// Marker for the Slow debuff. Applied to entities inside an active force field's Voronoi cell.
/// Persists as long as the entity remains in the same cell; no expiry.
/// Not saved — fully derived from field state, re-applied after load.
#[derive(Component)]
pub struct SlowEffect;

pub struct SlowEffectPlugin;
impl Plugin for SlowEffectPlugin {
    fn build(&self, app: &mut App) {
        app
            .add_observer(BuilderSlowEffect::on_add)
            ;
    }
}

#[derive(Component)]
pub struct BuilderSlowEffect {
    target_entity: Entity,
    source_entity: Option<Entity>,
    slow_amount: f32,
}
impl BuilderSlowEffect {
    pub fn new(target_entity: Entity, slow_amount: f32) -> Self {
        Self { target_entity, source_entity: None, slow_amount }
    }

    pub fn with_source(mut self, source_entity: Entity) -> Self {
        self.source_entity = Some(source_entity);
        self
    }

    fn on_add(
        trigger: On<Add, BuilderSlowEffect>,
        mut commands: Commands,
        builders: Query<&BuilderSlowEffect>,
    ) {
        let entity = trigger.entity;
        let Ok(builder) = builders.get(entity) else { return; };
        let target_entity = builder.target_entity;
        let source_entity = builder.source_entity;
        let slow_amount = builder.slow_amount;

        let mut entity_commands = commands.entity(entity);
        entity_commands
            .remove::<BuilderSlowEffect>()
            .insert((
                EffectTarget(target_entity),
                ModifierContributions(HashMap::from([(ModifierType::MovementSpeed, -slow_amount)])),
                SlowEffect,
                FieldEffect,
            ));
        if let Some(source_entity) = source_entity {
            entity_commands.insert(EffectSource(source_entity));
        }
    }
}
