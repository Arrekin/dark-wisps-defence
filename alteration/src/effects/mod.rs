pub mod visual;
pub mod brittle;
pub mod slow;

use bevy::platform::collections::HashMap;
use bevy::prelude::*;

use crate::modifiers::ModifierType;

pub mod prelude {
    pub use super::{
        BaselineEffect, EffectInstances, EffectSource, EffectSourceOf, EffectTarget,
        ExpiresAt, FieldEffect, ModifierContributions,
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
#[derive(Component)]
#[relationship(relationship_target = EffectSourceOf)]
pub struct EffectSource(pub Entity);

/// Inverse of `EffectSource`.
#[derive(Component, Default)]
#[relationship_target(relationship = EffectSource)]
pub struct EffectSourceOf(Vec<Entity>);

/////////////////////
// MODIFIER CONTRIBUTIONS //
/////////////////////

/// The set of stat contributions an effect instance provides to its target.
#[derive(Component)]
pub struct ModifierContributions(pub HashMap<ModifierType, f32>);

/// Marker on the permanent baseline effect instance spawned at entity creation.
///
/// Baseline effects are never saved; they are always reconstructed when the entity spawns.
#[derive(Component)]
pub struct BaselineEffect;

/// Marker on every effect entity spawned by a force field.
#[derive(Component)]
pub struct FieldEffect;

/////////////////
// EXPIRY      //
/////////////////

/// Absolute game-time at which the effect instance is despawned.
#[derive(Component, Clone, Copy, PartialEq)]
pub struct ExpiresAt(pub f64);
