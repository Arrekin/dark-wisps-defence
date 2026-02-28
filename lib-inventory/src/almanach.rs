use lib_core::{
    map_objects::{DarkOre, QuantumField, Wall},
    placement::{BeginPlacing, PlaceRequest, RemoveRequest},
};

use crate::{
    lib_prelude::*,
    placement::{
        GridsCollection, ObjectPlacementInfo, PlacementMode, PlacementValidationResult,
        PlacementValidatorFn,
    },
};

pub mod almanach_prelude {
    pub use super::{
        Almanach, AlmanachAppExt, AlmanachRegistrations, BuildingInfo, DarkOreInfo,
        QuantumFieldInfo, UpgradeInfo, UpgradeLevelInfo, WallInfo, WispInfo,
        building_validator,
    };
    pub use super::super::placement::{GridsCollection, PlacementValidationResult, PlacementValidatorFn, validate_empty_placement};
    pub use lib_core::placement::{PlaceRequest, RemoveRequest};
}

pub struct AlmanachPlugin;
impl Plugin for AlmanachPlugin {
    fn build(&self, app: &mut App) {
        app
            .init_resource::<AlmanachRegistrations>()
            .add_systems(OnEnter(MapLoadingStage::Init), Almanach::init_from_registrations);
    }
}

// ============================================================================
// ALMANACH REGISTRATIONS - Baseline collected at startup via AlmanachAppExt
// ============================================================================

#[derive(Resource, Default, Clone)]
pub struct AlmanachRegistrations {
    pub buildings: HashMap<BuildingType, BuildingInfo>,
    pub walls: Option<WallInfo>,
    pub dark_ore: Option<DarkOreInfo>,
    pub quantum_fields: Option<QuantumFieldInfo>,
    pub wisps: Option<WispInfo>,
}

pub trait AlmanachAppExt {
    fn register_building(&mut self, building_type: BuildingType, info: BuildingInfo) -> &mut Self;
    fn register_walls(&mut self, info: WallInfo) -> &mut Self;
    fn register_dark_ore(&mut self, info: DarkOreInfo) -> &mut Self;
    fn register_quantum_field(&mut self, info: QuantumFieldInfo) -> &mut Self;
    fn register_wisps(&mut self, info: WispInfo) -> &mut Self;
}

impl AlmanachAppExt for App {
    fn register_building(&mut self, building_type: BuildingType, info: BuildingInfo) -> &mut Self {
        self.init_resource::<AlmanachRegistrations>();
        self.world_mut().resource_mut::<AlmanachRegistrations>()
            .buildings.insert(building_type, info);
        self
    }

    fn register_walls(&mut self, info: WallInfo) -> &mut Self {
        self.init_resource::<AlmanachRegistrations>();
        self.world_mut().resource_mut::<AlmanachRegistrations>().walls = Some(info);
        self
    }

    fn register_dark_ore(&mut self, info: DarkOreInfo) -> &mut Self {
        self.init_resource::<AlmanachRegistrations>();
        self.world_mut().resource_mut::<AlmanachRegistrations>().dark_ore = Some(info);
        self
    }

    fn register_quantum_field(&mut self, info: QuantumFieldInfo) -> &mut Self {
        self.init_resource::<AlmanachRegistrations>();
        self.world_mut().resource_mut::<AlmanachRegistrations>().quantum_fields = Some(info);
        self
    }

    fn register_wisps(&mut self, info: WispInfo) -> &mut Self {
        self.init_resource::<AlmanachRegistrations>();
        self.world_mut().resource_mut::<AlmanachRegistrations>().wisps = Some(info);
        self
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
    pub wisps: WispInfo,
}

impl Almanach {
    fn init_from_registrations(mut commands: Commands, registrations: Res<AlmanachRegistrations>) {
        commands.insert_resource(Almanach {
            buildings: registrations.buildings.clone(),
            walls: registrations.walls.clone().expect("WallInfo not registered in AlmanachRegistrations"),
            dark_ore: registrations.dark_ore.clone().expect("DarkOreInfo not registered in AlmanachRegistrations"),
            quantum_fields: registrations.quantum_fields.clone().expect("QuantumFieldInfo not registered in AlmanachRegistrations"),
            wisps: registrations.wisps.clone().expect("WispInfo not registered in AlmanachRegistrations"),
        });
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

    /// Extracts generic ObjectPlacementInfo for any MapObject.
    pub fn get_placement_info_for(&self, map_object: MapObject) -> ObjectPlacementInfo {
        match map_object {
            MapObject::Building(building_type) => self.get_building_info(building_type).into(),
            MapObject::Wall => (&self.walls).into(),
            MapObject::DarkOre => (&self.dark_ore).into(),
            MapObject::QuantumField => (&self.quantum_fields).into(),
            MapObject::Wisp(_) => (&self.wisps).into(),
        }
    }
}


// ============================================================================
// BUILDING INFO
// ============================================================================

pub fn building_validator(map_object: MapObject, coords: GridCoords, imprint: GridImprint, map_data: &GridsCollection) -> PlacementValidationResult {
    let MapObject::Building(building_type) = map_object else {
        return PlacementValidationResult::invalid();
    };

    // Bounds check
    if !coords.is_imprint_in_bounds(&imprint, map_data.obstacle_grid.bounds()) {
        return PlacementValidationResult::invalid();
    }

    // Reserved check
    if map_data.reserved_coords.any_reserved(coords, imprint) {
        return PlacementValidationResult::invalid();
    }

    // Obstacle check
    if !map_data.obstacle_grid.query_building_placement(coords, building_type, imprint) {
        return PlacementValidationResult::invalid();
    }

    // Power check
    let needs_power = !matches!(building_type, BuildingType::MainBase | BuildingType::EnergyRelay);
    if needs_power && !map_data.energy_supply_grid.is_imprint_powered(coords, imprint) {
        return PlacementValidationResult::valid_unpowered();
    }

    PlacementValidationResult::valid()
}

#[derive(Clone)]
pub struct BuildingInfo {
    pub name: String,
    pub grid_imprint: GridImprint,
    pub cost: Vec<Cost>,
    pub baseline: HashMap<ModifierType, f32>,
    pub upgrades: HashMap<UpgradeType, UpgradeInfo>,
    pub validate: PlacementValidatorFn,
}

impl From<&BuildingInfo> for ObjectPlacementInfo {
    fn from(info: &BuildingInfo) -> Self {
        Self {
            imprint: info.grid_imprint,
            validate: info.validate,
            place_emitter: Box::new(PlaceRequest::<Building>::default()),
            remove_emitter: None,
            begin_placing_emitter: Some(Box::new(BeginPlacing::<Building>::default())),
            place_mode: PlacementMode::OnRelease,
            remove_mode: PlacementMode::OnRelease,
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

#[derive(Clone)]
pub struct WallInfo {
    pub name: String,
    pub grid_imprint: GridImprint,
    pub sprite_path: String,
    pub validate: PlacementValidatorFn,
}

impl From<&WallInfo> for ObjectPlacementInfo {
    fn from(info: &WallInfo) -> Self {
        Self {
            imprint: info.grid_imprint,
            validate: info.validate,
            place_emitter: Box::new(PlaceRequest::<Wall>::default()),
            remove_emitter: Some(Box::new(RemoveRequest::<Wall>::default())),
            begin_placing_emitter: Some(Box::new(BeginPlacing::<Wall>::default())),
            place_mode: PlacementMode::OnPress,
            remove_mode: PlacementMode::OnPress,
        }
    }
}

// ============================================================================
// DARK ORE INFO
// ============================================================================

#[derive(Clone)]
pub struct DarkOreInfo {
    pub name: String,
    pub grid_imprint: GridImprint,
    pub sprite_paths: Vec<String>,
    pub default_amount: u32,
    pub validate: PlacementValidatorFn,
}

impl From<&DarkOreInfo> for ObjectPlacementInfo {
    fn from(info: &DarkOreInfo) -> Self {
        Self {
            imprint: info.grid_imprint,
            validate: info.validate,
            place_emitter: Box::new(PlaceRequest::<DarkOre>::default()),
            remove_emitter: Some(Box::new(RemoveRequest::<DarkOre>::default())),
            begin_placing_emitter: Some(Box::new(BeginPlacing::<DarkOre>::default())),
            place_mode: PlacementMode::OnPress,
            remove_mode: PlacementMode::OnPress,
        }
    }
}

// ============================================================================
// QUANTUM FIELD INFO
// ============================================================================

#[derive(Clone)]
pub struct QuantumFieldInfo {
    pub name: String,
    pub min_size: i32,
    pub max_size: i32,
    pub default_size: i32,
    pub validate: PlacementValidatorFn,
}

impl From<&QuantumFieldInfo> for ObjectPlacementInfo {
    fn from(info: &QuantumFieldInfo) -> Self {
        Self {
            imprint: info.default_imprint(),
            validate: info.validate,
            place_emitter: Box::new(PlaceRequest::<QuantumField>::default()),
            remove_emitter: Some(Box::new(RemoveRequest::<QuantumField>::default())),
            begin_placing_emitter: Some(Box::new(BeginPlacing::<QuantumField>::default())),
            place_mode: PlacementMode::OnRelease,
            remove_mode: PlacementMode::OnRelease,
        }
    }
}

impl QuantumFieldInfo {
    pub fn default_imprint(&self) -> GridImprint {
        GridImprint::Rectangle { width: self.default_size, height: self.default_size }
    }
}

// ============================================================================
// WISP INFO
// ============================================================================

#[derive(Clone)]
pub struct WispInfo {
    pub grid_imprint: GridImprint,
    pub validate: PlacementValidatorFn,
}

impl From<&WispInfo> for ObjectPlacementInfo {
    fn from(info: &WispInfo) -> Self {
        Self {
            imprint: info.grid_imprint,
            validate: info.validate,
            place_emitter: Box::new(PlaceRequest::<WispType>::default()),
            remove_emitter: Some(Box::new(RemoveRequest::<WispType>::default())),
            begin_placing_emitter: None,
            place_mode: PlacementMode::OnPress,
            remove_mode: PlacementMode::OnPress,
        }
    }
}
