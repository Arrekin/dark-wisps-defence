use strum::{AsRefStr, EnumIter, EnumString, IntoEnumIterator};

use game_core::prelude::Shard;

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum ResourceType {
    DarkOre,
    Essence(EssenceType),
    Shard(Shard),
}

impl ResourceType {
    /// Every resource type, shards in every tier included.
    pub fn all() -> impl Iterator<Item = ResourceType> {
        std::iter::once(ResourceType::DarkOre)
            .chain(EssenceType::iter().map(ResourceType::Essence))
            .chain(Shard::all().map(ResourceType::Shard))
    }
}

impl From<Shard> for ResourceType {
    fn from(shard: Shard) -> Self {
        Self::Shard(shard)
    }
}

impl From<EssenceType> for ResourceType {
    fn from(essence_type: EssenceType) -> Self {
        Self::Essence(essence_type)
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct ResourceAmount {
    pub resource_type: ResourceType,
    pub amount: i32,
}
impl ResourceAmount {
    pub fn new(resource_type: impl Into<ResourceType>, amount: i32) -> Self {
        Self { resource_type: resource_type.into(), amount }
    }
}
impl<T: Into<ResourceType>> From<(T, i32)> for ResourceAmount {
    fn from((resource_type, amount): (T, i32)) -> Self {
        Self::new(resource_type, amount)
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, EnumString, AsRefStr, EnumIter)]
pub enum EssenceType {
    Fire,
    Water,
    Light,
    Electric,
}
