use bevy::prelude::*;

use alteration::effects::{ModifierContributions, prelude::{EffectSource, EffectTarget}};

/// Marker on effect entities spawned by a socketed shard, distinguishing them from baseline effects,
/// aura effects, debuffs, etc.
#[derive(Component)]
pub struct ShardEffect;
impl ShardEffect {
    /// Bundle for a shard effect that only contributes modifier values, with no behavioral markers.
    pub fn from_contributions(source: Entity, target: Entity, contributions: ModifierContributions) -> impl Bundle {
        (EffectSource(source), EffectTarget(target), contributions, ShardEffect)
    }
}
