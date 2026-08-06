use bevy::prelude::*;
use strum::{AsRefStr, Display, EnumIter, EnumString};

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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Display, EnumString, EnumIter, Default)]
pub enum ShardType {
    #[default]
    Range,
    Damage,
    Speed,
    Fire,
    Water,
    Light,
    Electric,
}
