use bevy::prelude::*;
use strum::{AsRefStr, Display, EnumIter, EnumString, IntoEnumIterator};

#[derive(Component, Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub enum BuildingType {
    EnergyRelay,
    MainBase,
    Tower(TowerType),
    MiningComplex,
    ExplorationCenter,
    Forge,
}
impl BuildingType {
    /// Returns all BuildingType variants including all tower types.
    pub fn all() -> impl Iterator<Item = Self> {
        [
            Self::MainBase,
            Self::EnergyRelay,
            Self::MiningComplex,
            Self::ExplorationCenter,
            Self::Forge,
            Self::Tower(TowerType::Blaster),
            Self::Tower(TowerType::Cannon),
            Self::Tower(TowerType::RocketLauncher),
            Self::Tower(TowerType::Emitter),
            Self::Tower(TowerType::Field),
        ].into_iter()
    }

    pub fn is_energy_supplier(&self) -> bool {
        matches!(self, BuildingType::MainBase | BuildingType::EnergyRelay)
    }
    /// EnergyRelay is considered a consumer as it cannot operate without energy supply
    pub fn is_energy_consumer(&self) -> bool {
        !matches!(self, BuildingType::MainBase)
    }
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub enum TowerType {
    Blaster,
    Cannon,
    RocketLauncher,
    Emitter,
    Field,
}

#[derive(Component, Copy, Clone, Debug, PartialEq, Eq, Hash, EnumString, EnumIter, AsRefStr)]
pub enum WispType {
    Fire,
    Water,
    Light,
    Electric,
}

/// Global identifier for all placeable objects on the map.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum MapObject {
    Building(BuildingType),
    Wall,
    DarkOre,
    QuantumField,
    Wisp(WispType),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Display, EnumString, EnumIter, AsRefStr, Default)]
pub enum ShardType {
    #[default]
    Strength,
    Speed,
    Reach,
    Fire,
    Water,
    Light,
    Electric,
}

/// Shard tier, T1 the lowest. Ordered: each tier above T1 is forged from shards of the tier below.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Display, EnumString, EnumIter, AsRefStr, Default)]
pub enum ShardTier {
    #[default]
    T1,
    T2,
    T3,
}
impl ShardTier {
    /// The tier this tier is forged from; `None` for the lowest tier.
    pub fn below(self) -> Option<Self> {
        match self {
            Self::T1 => None,
            Self::T2 => Some(Self::T1),
            Self::T3 => Some(Self::T2),
        }
    }
}

/// One concrete shard: its type and tier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct Shard {
    pub shard_type: ShardType,
    pub tier: ShardTier,
}
impl Shard {
    pub fn new(shard_type: ShardType, tier: ShardTier) -> Self {
        Self { shard_type, tier }
    }

    /// Every shard type in every tier.
    pub fn all() -> impl Iterator<Item = Self> {
        ShardType::iter().flat_map(|shard_type| ShardTier::iter().map(move |tier| Self::new(shard_type, tier)))
    }
}
impl std::fmt::Display for Shard {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} {}", self.shard_type, self.tier)
    }
}
