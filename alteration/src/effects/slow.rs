use bevy::prelude::*;

/// Marker for the Slow debuff. Applied to entities inside an active force field's Voronoi cell.
/// Persists as long as the entity remains in the same cell; no expiry.
/// Not saved — fully derived from field state, re-applied after load.
#[derive(Component)]
pub struct SlowEffect;

#[derive(Component)]
pub struct BuilderSlowEffect {
    pub target_entity: Entity,
    pub source_entity: Option<Entity>,
    pub slow_amount: f32,
}
impl BuilderSlowEffect {
    pub fn new(target_entity: Entity, slow_amount: f32) -> Self {
        Self { target_entity, source_entity: None, slow_amount }
    }

    pub fn with_source(mut self, source_entity: Entity) -> Self {
        self.source_entity = Some(source_entity);
        self
    }
}
