use std::time::Duration;

use bevy::prelude::*;

use alteration::modifiers::prelude::{AttackDamage, AttackRange, AttackSpeed, MaxIntegrityPoints, ModifierBank};
use game_core::prelude::{BuildingType, MapBound, TowerType, ZDepth};
use grids::{AutoGridTransformSync, prelude::{GridVersion, ObstacleGridObject}};

#[derive(Component, Default)]
pub struct Tower;

/// Spawns a side-menu tooltip for `building_type`, anchored to `anchor`.
#[derive(Component, Clone, Copy, Debug)]
pub struct BuilderBuildingSideMenuTooltip {
    pub anchor: Entity,
    pub building_type: BuildingType,
}

#[derive(Component, Clone, Debug, Default)]
#[require(AutoGridTransformSync, ZDepth::Building, MaxIntegrityPoints, MapBound, ObstacleGridObject = ObstacleGridObject::Building, ModifierBank)]
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

#[derive(Component)]
#[require(Tower, Building, BuildingType = BuildingType::Tower(TowerType::Flamethrower), AttackRange, AttackSpeed, AttackDamage, TowerShootingTimer, TowerWispTarget)]
pub struct TowerFlamethrower;

#[derive(Component)]
#[require(Tower, Building, BuildingType = BuildingType::Tower(TowerType::BlazeMortar), AttackRange, AttackSpeed, AttackDamage, TowerShootingTimer, TowerWispTarget)]
pub struct TowerBlazeMortar;

#[derive(Component)]
#[require(Tower, Building, BuildingType = BuildingType::Tower(TowerType::Inferno), AttackRange, AttackSpeed, TowerShootingTimer, TowerWispTarget)]
pub struct TowerInferno;

#[derive(Component, Clone, Default)]
#[require(AttackSpeed)]
pub struct TowerShootingTimer(pub Timer);
impl TowerShootingTimer {
    /// A timer `elapsed` seconds into its reload.
    pub fn from_elapsed(elapsed: f32) -> Self {
        let mut timer = Timer::default();
        timer.set_elapsed(Duration::from_secs_f32(elapsed));
        Self(timer)
    }
}

#[derive(Component, Default)]
pub enum TowerWispTarget {
    #[default]
    SearchForNewTarget,
    Wisp(Entity),
    NoValidTargets(GridVersion),
    /// The tower wants no target for now. Targeting skips it until the tower sets `SearchForNewTarget`.
    Suspended,
}

pub mod prelude {
    pub use super::{
        Building,
        BuilderBuildingSideMenuTooltip,
        EnergyRelay,
        ExplorationCenter,
        Forge,
        MainBase,
        MiningComplex,
        Tower,
        TowerBlaster,
        TowerBlazeMortar,
        TowerCannon,
        TowerEmitter,
        TowerField,
        TowerFlamethrower,
        TowerInferno,
        TowerRocketLauncher,
        TowerShootingTimer,
        TowerWispTarget,
    };
}
