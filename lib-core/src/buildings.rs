use std::time::Duration;

use crate::lib_prelude::*;

pub mod buildings_prelude {
    pub use super::*;
}

pub struct BuildingsPlugin;
impl Plugin for BuildingsPlugin {
    fn build(&self, app: &mut App) {
        app
            .add_observer(TowerShootingTimer::on_attack_speed_change);
    }
}

#[derive(Component, Copy, Clone, Debug, Deserialize, Serialize, Eq, PartialEq, Hash)]
pub enum BuildingType {
    EnergyRelay,
    MainBase,
    Tower(TowerType),
    MiningComplex,
    ExplorationCenter,
}
impl BuildingType {
    /// Returns all BuildingType variants including all tower types.
    pub fn all() -> impl Iterator<Item = Self> {
        [
            Self::MainBase,
            Self::EnergyRelay,
            Self::MiningComplex,
            Self::ExplorationCenter,
            Self::Tower(TowerType::Blaster),
            Self::Tower(TowerType::Cannon),
            Self::Tower(TowerType::RocketLauncher),
            Self::Tower(TowerType::Emitter),
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

#[derive(Copy, Clone, Debug, Deserialize, Serialize, Eq, PartialEq, Hash)]
pub enum TowerType {
    Blaster,
    Cannon,
    RocketLauncher,
    Emitter,
}

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

#[derive(Component, Default)]
pub struct Tower;

#[derive(Component)]
#[require(Building, BuildingType = BuildingType::Tower(TowerType::Blaster), AttackRange, AttackSpeed, AttackDamage, TowerShootingTimer, TowerWispTarget)]
pub struct TowerBlaster;

#[derive(Component)]
#[require(Building, BuildingType = BuildingType::Tower(TowerType::Cannon), AttackRange, AttackSpeed, AttackDamage, TowerShootingTimer, TowerWispTarget)]
pub struct TowerCannon;

#[derive(Component)]
#[require(Building, BuildingType = BuildingType::Tower(TowerType::RocketLauncher), AttackRange, AttackSpeed, AttackDamage, TowerShootingTimer, TowerWispTarget)]
pub struct TowerRocketLauncher;

#[derive(Component)]
#[require(Building, BuildingType = BuildingType::Tower(TowerType::Emitter), AttackRange, AttackSpeed, AttackDamage, TowerShootingTimer, TowerWispTarget)]
pub struct TowerEmitter;


#[derive(Component, Default)]
#[require(AttackSpeed)]
pub struct TowerShootingTimer(pub Timer);
impl TowerShootingTimer {
    fn on_attack_speed_change(
        trigger: On<Insert, AttackSpeed>,
        mut timers: Query<(&mut TowerShootingTimer, &AttackSpeed)>
    ) {
        let entity = trigger.entity;
        let Ok((mut timer, attack_speed)) = timers.get_mut(entity) else { return; };
        if attack_speed.get() == 0. { return; }
        timer.0.set_duration(Duration::from_secs_f32(1. / attack_speed.get()));
    }
}

#[derive(Component, Default)]
pub enum TowerWispTarget {
    #[default]
    SearchForNewTarget,
    Wisp(Entity),
    NoValidTargets(GridVersion),
}

#[derive(Component, Default)]
pub struct DisabledByPlayer;