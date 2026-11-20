pub mod visual;
pub mod brittle;
pub mod slow;

use bevy::prelude::*;
use smallvec::SmallVec;

use crate::modifiers::ModifierType;

pub mod prelude {
    pub use super::{
        BaselineEffect, EffectInstances, EffectSource, EffectSourceOf, EffectTarget,
        FieldEffect, ModifierContributions,
    };
    pub use super::visual::{
        EFFECT_VISUAL_SLOTS,
        EffectVisualState,
    };
    pub use super::brittle::BuilderBrittleEffect;
    pub use super::slow::BuilderSlowEffect;
}

////////////////////////////
// EFFECT TARGET RELATION //
////////////////////////////

/// Relationship from an effect instance entity to the entity whose stats it modifies.
#[derive(Component)]
#[relationship(relationship_target = EffectInstances)]
pub struct EffectTarget(pub Entity);

/// Inverse of `EffectTarget`.
///
/// `linked_spawn` ensures that when this entity is despawned, all effect instances
/// targeting it are cascade-despawned.
#[derive(Component, Default)]
#[relationship_target(relationship = EffectTarget, linked_spawn)]
pub struct EffectInstances(Vec<Entity>);

////////////////////////////
// EFFECT SOURCE RELATION //
////////////////////////////

/// Relationship from an effect instance entity to the entity that spawned it.
#[derive(Component, Clone, Copy)]
#[relationship(relationship_target = EffectSourceOf)]
pub struct EffectSource(pub Entity);

/// Inverse of `EffectSource`.
#[derive(Component, Default)]
#[relationship_target(relationship = EffectSource)]
pub struct EffectSourceOf(Vec<Entity>);

////////////////////////////
// MODIFIER CONTRIBUTIONS //
////////////////////////////

/// The values an effect instance contributes to its target's modifiers, at most one per modifier.
///
/// Construction does not check for repeats, and a repeated modifier gives an unspecified outcome.
/// Insert a new `ModifierContributions` to change an effect's contributions.
#[derive(Component, Clone, Default)]
#[component(immutable)]
pub struct ModifierContributions(SmallVec<[(ModifierType, f32); 4]>);
impl ModifierContributions {
    pub fn push(&mut self, modifier: ModifierType, value: f32) {
        self.0.push((modifier, value));
    }

    pub fn iter(&self) -> impl Iterator<Item = (ModifierType, f32)> {
        self.0.iter().copied()
    }

    pub fn get(&self, modifier: ModifierType) -> Option<f32> {
        self.0.iter().find(|(entry_modifier, _)| *entry_modifier == modifier).map(|&(_, value)| value)
    }
}
impl FromIterator<(ModifierType, f32)> for ModifierContributions {
    fn from_iter<I: IntoIterator<Item = (ModifierType, f32)>>(entries: I) -> Self {
        Self(entries.into_iter().collect())
    }
}
impl<const N: usize> From<[(ModifierType, f32); N]> for ModifierContributions {
    fn from(entries: [(ModifierType, f32); N]) -> Self {
        entries.into_iter().collect()
    }
}

/// Marker on the permanent baseline effect instance spawned at entity creation.
///
/// Baseline effects are never saved; they are always reconstructed when the entity spawns.
#[derive(Component)]
pub struct BaselineEffect;

/// Marker on every effect entity spawned by a force field.
#[derive(Component)]
pub struct FieldEffect;
