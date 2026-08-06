use std::fmt;

use bevy::prelude::*;
use strum::{AsRefStr, EnumIter, EnumString};

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum ResourceType {
    DarkOre,
    Essence(EssenceType),
}

impl ResourceType {
    pub fn iter() -> [ResourceType; 5] {
        [
            ResourceType::DarkOre,
            ResourceType::Essence(EssenceType::Fire),
            ResourceType::Essence(EssenceType::Water),
            ResourceType::Essence(EssenceType::Light),
            ResourceType::Essence(EssenceType::Electric),
        ]
    }

    pub fn icon_path(self) -> &'static str {
        match self {
            ResourceType::DarkOre => "indicators/no_dark_ore.png",
            ResourceType::Essence(EssenceType::Fire) => "ui/shards/shard_fire.png",
            ResourceType::Essence(EssenceType::Water) => "ui/shards/shard_water.png",
            ResourceType::Essence(EssenceType::Light) => "ui/shards/shard_light.png",
            ResourceType::Essence(EssenceType::Electric) => "ui/shards/shard_electric.png",
        }
    }
}

impl fmt::Display for ResourceType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ResourceType::DarkOre => write!(f, "Dark Ore"),
            ResourceType::Essence(essence) => write!(f, "{} Essence", essence.as_ref()),
        }
    }
}

impl From<EssenceType> for ResourceType {
    fn from(essence_type: EssenceType) -> Self {
        Self::Essence(essence_type)
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, EnumString, AsRefStr, EnumIter)]
pub enum EssenceType {
    Fire,
    Water,
    Light,
    Electric,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct EssenceContainer {
    pub essence_type: EssenceType,
    pub amount: i32,
}
impl EssenceContainer {
    pub fn new(essence_type: EssenceType, amount: i32) -> Self {
        Self { essence_type, amount }
    }
}


#[derive(Component)]
pub struct EssencesContainer(pub Vec<EssenceContainer>);
impl From<EssenceContainer> for EssencesContainer {
    fn from(essence_container: EssenceContainer) -> Self {
        Self(vec![essence_container])
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct Cost {
    pub resource_type: ResourceType,
    pub amount: i32,
}
