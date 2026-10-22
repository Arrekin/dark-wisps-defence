use bevy::prelude::*;
use strum::{AsRefStr, Display, EnumCount, EnumIter, EnumString, IntoEnumIterator};

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
        ].into_iter().chain(TowerType::iter().map(Self::Tower))
    }

    pub fn is_energy_supplier(&self) -> bool {
        matches!(self, BuildingType::MainBase | BuildingType::EnergyRelay)
    }
    /// EnergyRelay is considered a consumer as it cannot operate without energy supply
    pub fn is_energy_consumer(&self) -> bool {
        !matches!(self, BuildingType::MainBase)
    }

    /// GPU building-type index, mirrored by `BUILDING_*` and `TOWER_*` in
    /// `assets/shaders/grids/obstacles.wesl`. Zero means no building; tower indices start at 32.
    pub fn gpu_index(self) -> u32 {
        match self {
            Self::MainBase => 1,
            Self::EnergyRelay => 2,
            Self::MiningComplex => 3,
            Self::ExplorationCenter => 4,
            Self::Forge => 5,
            Self::Tower(tower_type) => 32 + match tower_type {
                TowerType::Blaster => 0,
                TowerType::Cannon => 1,
                TowerType::RocketLauncher => 2,
                TowerType::Emitter => 3,
                TowerType::Field => 4,
                TowerType::Flamethrower => 5,
                TowerType::BlazeMortar => 6,
                TowerType::Inferno => 7,
                TowerType::Crucible => 8,
                TowerType::NapalmBattery => 9,
            },
        }
    }
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash, EnumIter)]
pub enum TowerType {
    Blaster,
    Cannon,
    RocketLauncher,
    Emitter,
    Field,
    Flamethrower,
    BlazeMortar,
    Inferno,
    Crucible,
    NapalmBattery,
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
impl ShardType {
    pub const STATS: [ShardType; 3] = [ShardType::Strength, ShardType::Speed, ShardType::Reach];
}

/// Shard tier, T1 the lowest. Ordered: each tier above T1 is forged from shards of the tier below.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Display, EnumString, EnumIter, AsRefStr, Default, EnumCount)]
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

#[cfg(test)]
mod tests {
    use bevy::platform::collections::HashSet;

    use super::*;

    #[test]
    fn building_gpu_indices_are_unique_and_fit_a_byte() {
        let mut seen = HashSet::new();
        for building_type in BuildingType::all() {
            let index = building_type.gpu_index();
            assert!(index != 0 && index < 256, "{building_type:?} has GPU index {index} outside 1..256");
            assert!(seen.insert(index), "{building_type:?} shares GPU index {index}");
        }
    }
}
