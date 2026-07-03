use bevy::prelude::*;
use strum::{AsRefStr, EnumIter, EnumString};

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum ResourceType {
    DarkOre,
    Essence(EssenceType),
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
