use bevy::prelude::*;

use game_core::prelude::SSS;

use super::ExpiresAt;
use super::visual::{EffectVisualContribution, BRITTLE_BIT, BRITTLE_SLOT};

/// Marker for the Brittle debuff. Applied to wisps hit by an emitter tower ripple.
/// Causes them to take increased incoming damage for a duration set by the source.
#[derive(Component)]
#[require(EffectVisualContribution = EffectVisualContribution::new(BRITTLE_BIT, BRITTLE_SLOT, Vec4::ZERO))]
pub struct BrittleEffect;

#[derive(Component, SSS)]
pub struct BuilderBrittleEffect {
    pub target_entity: Entity,
    pub source_entity: Option<Entity>,
    pub damage_multiplier: f32,
    pub expires_at: Option<ExpiresAt>,
}
impl BuilderBrittleEffect {
    pub fn new(target_entity: Entity, damage_multiplier: f32) -> Self {
        Self { target_entity, source_entity: None, damage_multiplier, expires_at: None }
    }

    pub fn with_source(mut self, source_entity: Entity) -> Self {
        self.source_entity = Some(source_entity);
        self
    }

    pub fn with_expiry(mut self, expires_at: ExpiresAt) -> Self {
        self.expires_at = Some(expires_at);
        self
    }
}
