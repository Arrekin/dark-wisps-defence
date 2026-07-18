use bevy::prelude::*;

use alteration::modifiers::prelude::{AttackDamage, AttackRange, AttackSpeed, MaxIntegrityPoints, ModifierBank};
use game_core::prelude::{BuildingType, MapBound, TowerType, Z_BUILDING, ZDepth};
use grids::{AutoGridTransformSync, prelude::{GridVersion, ObstacleGridObject}};

#[derive(Component, Default)]
pub struct Tower;

#[derive(Component, Clone, Debug, Default)]
#[require(AutoGridTransformSync, ZDepth = Z_BUILDING, MaxIntegrityPoints, MapBound, ObstacleGridObject = ObstacleGridObject::Building, ModifierBank)]
pub struct Building;

#[derive(Component)]
#[require(Building, BuildingType = BuildingType::MainBase)]
pub struct MainBase;

#[derive(Component)]
#[require(Building, BuildingType = BuildingType::EnergyRelay)]
pub struct EnergyRelay;

#[derive(Component)]
#[require(Building, BuildingType = BuildingType::MiningComplex)]
pub struct MiningComplex;

#[derive(Component)]
#[require(Building, BuildingType = BuildingType::Forge)]
pub struct Forge;

/// Building that owns and manages expedition drones.
/// Drones link back via HomeBase relationship; this component tracks capacity.
#[derive(Component)]
#[require(Building, BuildingType = BuildingType::ExplorationCenter)]
pub struct ExplorationCenter {
    pub max_drone_slots: usize,
}
impl ExplorationCenter {
    pub fn new(max_drone_slots: usize) -> Self {
        Self { max_drone_slots }
    }
}

#[derive(Component)]
#[require(Tower, Building, BuildingType = BuildingType::Tower(TowerType::Blaster), AttackRange, AttackSpeed, AttackDamage, TowerShootingTimer, TowerWispTarget)]
pub struct TowerBlaster;

#[derive(Component)]
#[require(Tower, Building, BuildingType = BuildingType::Tower(TowerType::Cannon), AttackRange, AttackSpeed, AttackDamage, TowerShootingTimer, TowerWispTarget)]
pub struct TowerCannon;

#[derive(Component)]
#[require(Tower, Building, BuildingType = BuildingType::Tower(TowerType::RocketLauncher), AttackRange, AttackSpeed, AttackDamage, TowerShootingTimer, TowerWispTarget)]
pub struct TowerRocketLauncher;

#[derive(Component)]
#[require(Tower, Building, BuildingType = BuildingType::Tower(TowerType::Emitter), AttackRange, AttackSpeed, AttackDamage, TowerShootingTimer, TowerWispTarget)]
pub struct TowerEmitter;

#[derive(Component)]
#[require(Tower, Building, BuildingType = BuildingType::Tower(TowerType::Field))]
pub struct TowerField;


#[derive(Component, Default)]
#[require(AttackSpeed)]
pub struct TowerShootingTimer(pub Timer);

#[derive(Component, Default)]
pub enum TowerWispTarget {
    #[default]
    SearchForNewTarget,
    Wisp(Entity),
    NoValidTargets(GridVersion),
}

pub mod prelude {
    pub use super::{
        Building,
        EnergyRelay,
        ExplorationCenter,
        Forge,
        MainBase,
        MiningComplex,
        Tower,
        TowerBlaster,
        TowerCannon,
        TowerEmitter,
        TowerField,
        TowerRocketLauncher,
        TowerShootingTimer,
        TowerWispTarget,
    };
}
