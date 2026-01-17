use crate::lib_prelude::*;

pub mod almanach_prelude {
    pub use super::{
        Almanach, 
        BuildingInfo, UpgradeInfo, UpgradeLevelInfo,
        WallInfo, DarkOreInfo, QuantumFieldInfo,
    };
}

pub struct AlmanachPlugin;
impl Plugin for AlmanachPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(OnEnter(MapLoadingStage::Init), |mut commands: Commands| { 
            commands.insert_resource(Almanach::new()); 
        });
    }
}

// ============================================================================
// ALMANACH - Central metadata store for all game objects
// ============================================================================

#[derive(Resource)]
pub struct Almanach {
    buildings: HashMap<BuildingType, BuildingInfo>,
    pub walls: WallInfo,
    pub dark_ore: DarkOreInfo,
    pub quantum_fields: QuantumFieldInfo,
}

impl Almanach {
    /// Creates a new Almanach populated with default values for all object types.
    pub fn new() -> Self {
        let mut buildings = HashMap::default();
        for building_type in BuildingType::all() {
            buildings.insert(building_type, BuildingInfo::default_for(building_type));
        }

        Self {
            buildings,
            walls: WallInfo::default(),
            dark_ore: DarkOreInfo::default(),
            quantum_fields: QuantumFieldInfo::default(),
        }
    }

    // === Buildings ===
    
    pub fn get_building_info(&self, building_type: BuildingType) -> &BuildingInfo {
        self.buildings.get(&building_type)
            .expect(&format!("Building {building_type:?} not found in almanach"))
    }

    pub fn get_building_info_mut(&mut self, building_type: BuildingType) -> &mut BuildingInfo {
        self.buildings.get_mut(&building_type)
            .expect(&format!("Building {building_type:?} not found in almanach"))
    }
}

impl Default for Almanach {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// BUILDING INFO
// ============================================================================

#[derive(Clone, Serialize, Deserialize)]
pub struct BuildingInfo {
    pub name: String,
    pub grid_imprint: GridImprint,
    pub cost: Vec<Cost>,
    pub baseline: HashMap<ModifierType, f32>,
    pub upgrades: HashMap<UpgradeType, UpgradeInfo>,
}

impl BuildingInfo {
    /// Returns the default BuildingInfo for a specific building type.
    pub fn default_for(building_type: BuildingType) -> Self {
        match building_type {
            BuildingType::MainBase => Self {
                name: "Main Base".to_string(),
                grid_imprint: GridImprint::Rectangle { width: 6, height: 6 },
                cost: vec![],
                baseline: HashMap::from([
                    (ModifierType::MaxHealth, 10000.),
                    (ModifierType::EnergySupplyRange, 15.),
                ]),
                upgrades: HashMap::default(),
            },
            BuildingType::EnergyRelay => Self {
                name: "Energy Relay".to_string(),
                grid_imprint: GridImprint::Rectangle { width: 2, height: 2 },
                cost: vec![Cost { resource_type: ResourceType::DarkOre, amount: 300 }],
                baseline: HashMap::from([
                    (ModifierType::MaxHealth, 100.),
                    (ModifierType::EnergySupplyRange, 12.),
                ]),
                upgrades: HashMap::default(),
            },
            BuildingType::MiningComplex => Self {
                name: "Mining Complex".to_string(),
                grid_imprint: GridImprint::Rectangle { width: 3, height: 3 },
                cost: vec![Cost { resource_type: ResourceType::DarkOre, amount: 100 }],
                baseline: HashMap::from([
                    (ModifierType::MaxHealth, 100.),
                ]),
                upgrades: HashMap::default(),
            },
            BuildingType::ExplorationCenter => Self {
                name: "Exploration Center".to_string(),
                grid_imprint: GridImprint::Rectangle { width: 4, height: 4 },
                cost: vec![Cost { resource_type: ResourceType::DarkOre, amount: 500 }],
                baseline: HashMap::from([
                    (ModifierType::MaxHealth, 100.),
                ]),
                upgrades: HashMap::default(),
            },
            BuildingType::Tower(TowerType::Blaster) => Self {
                name: "Blaster Tower".to_string(),
                grid_imprint: GridImprint::Rectangle { width: 2, height: 2 },
                cost: vec![Cost { resource_type: ResourceType::DarkOre, amount: 150 }],
                baseline: HashMap::from([
                    (ModifierType::MaxHealth, 100.),
                    (ModifierType::AttackRange, 15.),
                    (ModifierType::AttackSpeed, 5.),
                    (ModifierType::AttackDamage, 1.),
                ]),
                upgrades: HashMap::from([
                    (UpgradeType::Modifier(ModifierType::AttackSpeed), UpgradeInfo {
                        levels: vec![
                            UpgradeLevelInfo { value: 0.1, cost: vec![Cost { resource_type: ResourceType::DarkOre, amount: 100 }] },
                            UpgradeLevelInfo { value: 0.1, cost: vec![Cost { resource_type: ResourceType::DarkOre, amount: 200 }] },
                            UpgradeLevelInfo { value: 0.1, cost: vec![Cost { resource_type: ResourceType::DarkOre, amount: 300 }] },
                        ],
                    }),
                    (UpgradeType::Modifier(ModifierType::AttackDamage), UpgradeInfo {
                        levels: vec![
                            UpgradeLevelInfo { value: 1., cost: vec![Cost { resource_type: ResourceType::DarkOre, amount: 100 }] },
                            UpgradeLevelInfo { value: 1., cost: vec![Cost { resource_type: ResourceType::DarkOre, amount: 200 }] },
                            UpgradeLevelInfo { value: 1., cost: vec![Cost { resource_type: ResourceType::DarkOre, amount: 300 }] },
                        ],
                    }),
                ]),
            },
            BuildingType::Tower(TowerType::Cannon) => Self {
                name: "Cannon Tower".to_string(),
                grid_imprint: GridImprint::Rectangle { width: 3, height: 3 },
                cost: vec![Cost { resource_type: ResourceType::DarkOre, amount: 250 }],
                baseline: HashMap::from([
                    (ModifierType::MaxHealth, 100.),
                    (ModifierType::AttackRange, 15.),
                    (ModifierType::AttackSpeed, 0.5),
                    (ModifierType::AttackDamage, 50.),
                ]),
                upgrades: HashMap::from([
                    (UpgradeType::Modifier(ModifierType::AttackRange), UpgradeInfo {
                        levels: vec![
                            UpgradeLevelInfo { value: 1., cost: vec![Cost { resource_type: ResourceType::DarkOre, amount: 100 }] },
                            UpgradeLevelInfo { value: 2., cost: vec![Cost { resource_type: ResourceType::DarkOre, amount: 200 }] },
                            UpgradeLevelInfo { value: 3., cost: vec![Cost { resource_type: ResourceType::DarkOre, amount: 300 }] },
                        ],
                    }),
                    (UpgradeType::Modifier(ModifierType::AttackDamage), UpgradeInfo {
                        levels: vec![
                            UpgradeLevelInfo { value: 5., cost: vec![Cost { resource_type: ResourceType::DarkOre, amount: 100 }] },
                            UpgradeLevelInfo { value: 10., cost: vec![Cost { resource_type: ResourceType::DarkOre, amount: 200 }] },
                            UpgradeLevelInfo { value: 15., cost: vec![Cost { resource_type: ResourceType::DarkOre, amount: 300 }] },
                        ],
                    }),
                ]),
            },
            BuildingType::Tower(TowerType::RocketLauncher) => Self {
                name: "Rocket Launcher Tower".to_string(),
                grid_imprint: GridImprint::Rectangle { width: 3, height: 3 },
                cost: vec![Cost { resource_type: ResourceType::DarkOre, amount: 350 }],
                baseline: HashMap::from([
                    (ModifierType::MaxHealth, 100.),
                    (ModifierType::AttackRange, 30.),
                    (ModifierType::AttackSpeed, 0.33),
                    (ModifierType::AttackDamage, 50.),
                ]),
                upgrades: HashMap::from([
                    (UpgradeType::Modifier(ModifierType::AttackRange), UpgradeInfo {
                        levels: vec![
                            UpgradeLevelInfo { value: 2., cost: vec![Cost { resource_type: ResourceType::DarkOre, amount: 100 }] },
                            UpgradeLevelInfo { value: 2., cost: vec![Cost { resource_type: ResourceType::DarkOre, amount: 200 }] },
                            UpgradeLevelInfo { value: 5., cost: vec![Cost { resource_type: ResourceType::DarkOre, amount: 300 }] },
                        ],
                    }),
                    (UpgradeType::Modifier(ModifierType::AttackSpeed), UpgradeInfo {
                        levels: vec![
                            UpgradeLevelInfo { value: 0.1, cost: vec![Cost { resource_type: ResourceType::DarkOre, amount: 100 }] },
                            UpgradeLevelInfo { value: 0.1, cost: vec![Cost { resource_type: ResourceType::DarkOre, amount: 200 }] },
                            UpgradeLevelInfo { value: 0.3, cost: vec![Cost { resource_type: ResourceType::DarkOre, amount: 300 }] },
                        ],
                    }),
                ]),
            },
            BuildingType::Tower(TowerType::Emitter) => Self {
                name: "Emitter Tower".to_string(),
                grid_imprint: GridImprint::Rectangle { width: 2, height: 2 },
                cost: vec![Cost { resource_type: ResourceType::DarkOre, amount: 450 }],
                baseline: HashMap::from([
                    (ModifierType::MaxHealth, 100.),
                    (ModifierType::AttackRange, 4.),
                    (ModifierType::AttackSpeed, 0.5),
                    (ModifierType::AttackDamage, 1.),
                ]),
                upgrades: HashMap::from([
                    (UpgradeType::Modifier(ModifierType::AttackRange), UpgradeInfo {
                        levels: vec![
                            UpgradeLevelInfo { value: 1., cost: vec![Cost { resource_type: ResourceType::DarkOre, amount: 100 }] },
                            UpgradeLevelInfo { value: 2., cost: vec![Cost { resource_type: ResourceType::DarkOre, amount: 200 }] },
                            UpgradeLevelInfo { value: 3., cost: vec![Cost { resource_type: ResourceType::DarkOre, amount: 300 }] },
                        ],
                    }),
                    (UpgradeType::Modifier(ModifierType::AttackSpeed), UpgradeInfo {
                        levels: vec![
                            UpgradeLevelInfo { value: 0.1, cost: vec![Cost { resource_type: ResourceType::DarkOre, amount: 100 }] },
                            UpgradeLevelInfo { value: 0.1, cost: vec![Cost { resource_type: ResourceType::DarkOre, amount: 200 }] },
                            UpgradeLevelInfo { value: 0.1, cost: vec![Cost { resource_type: ResourceType::DarkOre, amount: 300 }] },
                        ],
                    }),
                ]),
            },
        }
    }
}

#[derive(Clone, Serialize, Deserialize)]
pub struct UpgradeInfo {
    pub levels: Vec<UpgradeLevelInfo>,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct UpgradeLevelInfo {
    pub cost: Vec<Cost>,
    pub value: f32,
}

// ============================================================================
// WALL INFO
// ============================================================================

#[derive(Clone, Serialize, Deserialize)]
pub struct WallInfo {
    pub name: String,
    pub grid_imprint: GridImprint,
    pub sprite_path: String,
}

impl Default for WallInfo {
    fn default() -> Self {
        Self {
            name: "Wall".to_string(),
            grid_imprint: GridImprint::Rectangle { width: 1, height: 1 },
            sprite_path: "map_objects/wall_4side.png".to_string(),
        }
    }
}

// ============================================================================
// DARK ORE INFO
// ============================================================================

#[derive(Clone, Serialize, Deserialize)]
pub struct DarkOreInfo {
    pub name: String,
    pub grid_imprint: GridImprint,
    pub sprite_paths: Vec<String>,
    pub default_amount: u32,
}

impl Default for DarkOreInfo {
    fn default() -> Self {
        Self {
            name: "Dark Ore".to_string(),
            grid_imprint: GridImprint::Rectangle { width: 1, height: 1 },
            sprite_paths: vec![
                "map_objects/dark_ore_1.png".to_string(),
                "map_objects/dark_ore_2.png".to_string(),
            ],
            default_amount: 1000,
        }
    }
}

// ============================================================================
// QUANTUM FIELD INFO
// ============================================================================

#[derive(Clone, Serialize, Deserialize)]
pub struct QuantumFieldInfo {
    pub name: String,
    pub min_size: i32,
    pub max_size: i32,
    pub default_size: i32,
}

impl Default for QuantumFieldInfo {
    fn default() -> Self {
        Self {
            name: "Quantum Field".to_string(),
            min_size: 3,
            max_size: 6,
            default_size: 3,
        }
    }
}

impl QuantumFieldInfo {
    pub fn default_imprint(&self) -> GridImprint {
        GridImprint::Rectangle { width: self.default_size, height: self.default_size }
    }
}
