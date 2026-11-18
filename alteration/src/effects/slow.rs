use bevy::prelude::*;

use super::EffectSource;

/// Marker for the Slow debuff.
#[derive(Component)]
pub struct SlowEffect;

#[derive(Component)]
pub struct BuilderSlowEffect {
    pub target_entity: Entity,
    pub source_entity: Option<EffectSource>,
    pub slow_percentage: f32,
}
impl BuilderSlowEffect {
    pub fn new(target_entity: Entity, slow_percentage: f32) -> Self {
        Self { target_entity, source_entity: None, slow_percentage }
    }

    pub fn with_source(mut self, source_entity: Entity) -> Self {
        self.source_entity = Some(EffectSource(source_entity));
        self
    }
}
