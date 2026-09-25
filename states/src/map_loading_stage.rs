use bevy::prelude::*;
use serde::Serialize;

#[derive(Default, Clone, Debug, States, PartialEq, Eq, Hash, Serialize)]
pub enum MapLoadingStage {
    #[default]
    Init,
    LoadMapInfo,
    LoadResources,
    SpawnMapElements,
    SpawnEffectInstances,
    Ready,
}
impl MapLoadingStage {
    pub fn next(&self) -> Option<Self> {
        match self {
            MapLoadingStage::Init => Some(MapLoadingStage::LoadMapInfo),
            MapLoadingStage::LoadMapInfo => Some(MapLoadingStage::LoadResources),
            MapLoadingStage::LoadResources => Some(MapLoadingStage::SpawnMapElements),
            MapLoadingStage::SpawnMapElements => Some(MapLoadingStage::SpawnEffectInstances),
            MapLoadingStage::SpawnEffectInstances => Some(MapLoadingStage::Ready),
            MapLoadingStage::Ready => None,
        }
    }
}
