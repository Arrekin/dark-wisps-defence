use bevy::prelude::*;

use game_core::prelude::SSS;
use session::TimeBound;

use super::EffectSource;
use super::visual::{EffectVisualContribution, BRITTLE_BIT, BRITTLE_SLOT};

/// Marker for the Brittle debuff. Applied to wisps hit by an emitter tower ripple.
/// Causes them to take increased incoming damage for a duration set by the source.
#[derive(Component)]
#[require(EffectVisualContribution = EffectVisualContribution::new(BRITTLE_BIT, BRITTLE_SLOT, Vec4::ZERO))]
pub struct BrittleEffect;

#[derive(Component, SSS)]
pub struct BuilderBrittleEffect {
    pub target_entity: Entity,
    pub source_entity: Option<EffectSource>,
    pub damage_multiplier: f32,
    pub time_bound: Option<TimeBound>,
}
impl BuilderBrittleEffect {
    pub fn new(target_entity: Entity, damage_multiplier: f32) -> Self {
        Self { target_entity, source_entity: None, damage_multiplier, time_bound: None }
    }

    pub fn with_source(mut self, source_entity: impl Into<Option<Entity>>) -> Self {
        self.source_entity = source_entity.into().map(EffectSource);
        self
    }

    pub fn with_time_bound(mut self, time_bound: impl Into<Option<TimeBound>>) -> Self {
        self.time_bound = time_bound.into();
        self
    }
}
