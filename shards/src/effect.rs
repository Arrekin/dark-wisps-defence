use bevy::platform::collections::HashMap;
use bevy::prelude::*;

use alteration::{
    effects::{ModifierContributions, prelude::EffectTarget},
    modifiers::prelude::ModifierType,
};

/// Marker on effect entities spawned by shard application.
///
/// Distinguishes shard effects from baseline effects, aura effects, debuffs, etc.
/// Used for removal queries, save/load filtering, and UI.
#[derive(Component)]
pub struct ShardEffect;
impl ShardEffect {
    /// Returns a bundle for a stat-only shard effect. Convenience for the common case
    /// where a shard simply contributes modifier values with no behavioral markers.
    pub fn from_modifiers(shard_target: Entity, contributions: HashMap<ModifierType, f32>) -> impl Bundle {
        (EffectTarget(shard_target), ModifierContributions(contributions), ShardEffect)
    }
}
