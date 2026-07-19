use bevy::ecs::entity_disabling::Disabled;
use bevy::prelude::*;

use game_core::prelude::ZDepth;

// Marks the root of the space allowed for external content.
#[derive(Component)]
pub struct DisplayPanelMainContentRoot;

/// Marker placed on the currently selected map object.
/// Insert to select, remove to deselect.
/// Observers on `On<Insert, FocusedMapObject>` and `On<Remove, FocusedMapObject>` drive
/// overlay highlights, info panel content, and other selection-dependent UI.
#[derive(Component)]
pub struct FocusedMapObject;

#[derive(Component)]
#[relationship(relationship_target = Indicators)]
pub struct IndicatorOf(pub Entity);

#[derive(Component)]
#[relationship_target(relationship = IndicatorOf, linked_spawn)]
pub struct Indicators(Vec<Entity>);
impl Indicators {
    pub fn entities(&self) -> &Vec<Entity> {
        &self.0
    }
}

#[derive(Component)]
#[require(Transform, Visibility, Sprite, ZDepth::MAP_UI)]
pub struct IndicatorDisplay {
    pub active_index: usize,
    pub cycle_time: f32,
}
impl Default for IndicatorDisplay {
    fn default() -> Self {
        Self {
            active_index: 0,
            cycle_time: 0.0,
        }
    }
}

#[derive(Component, Default)]
pub struct IndicatorSpriteHandle(pub Handle<Image>);

#[derive(Component, Clone, Copy, PartialEq, Eq)]
#[component(immutable)]
#[require(IndicatorSpriteHandle, Disabled)]
pub enum IndicatorType {
    NoPower,
    OreDepleted,
    DisabledByPlayer,
}

pub mod prelude {
    pub use super::{
        DisplayPanelMainContentRoot, FocusedMapObject, IndicatorDisplay, IndicatorOf,
        IndicatorSpriteHandle, IndicatorType, Indicators,
    };
}
