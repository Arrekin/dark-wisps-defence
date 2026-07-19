use bevy::platform::collections::HashMap;
use bevy::prelude::*;

use alteration::modifiers::prelude::ModifierType;
use game_core::prelude::{BuildingType, GridImprint, MapObject, ShardType};
use grids::placement::{ObjectPlacementInfo, PlacementAnnotatorFn, PlacementChannel, PlacementValidatorFn};
use resources::prelude::Cost;
use states::prelude::MapLoadingStage;

pub mod prelude {
    pub use super::{Almanach, AlmanachAppExt, BuildingInfo, ShardInfo, ShardRecipe};
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
    pub shards: HashMap<ShardType, ShardInfo>,
    pub walls: Option<WallInfo>,
    pub dark_ore: Option<DarkOreInfo>,
    pub quantum_fields: Option<QuantumFieldInfo>,
    pub wisps: Option<WispInfo>,
}

pub trait AlmanachAppExt {
    fn register_building(&mut self, building_type: BuildingType, info: BuildingInfo) -> &mut Self;
    fn register_shard(&mut self, shard_type: ShardType, info: ShardInfo) -> &mut Self;
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

    fn register_shard(&mut self, shard_type: ShardType, info: ShardInfo) -> &mut Self {
        self.init_resource::<AlmanachRegistrations>();
        self.world_mut().resource_mut::<AlmanachRegistrations>()
            .shards.insert(shard_type, info);
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
    shards: HashMap<ShardType, ShardInfo>,
    pub walls: WallInfo,
    pub dark_ore: DarkOreInfo,
    pub quantum_fields: QuantumFieldInfo,
    pub wisps: WispInfo,
}

impl Almanach {
    fn init_from_registrations(mut commands: Commands, registrations: Res<AlmanachRegistrations>) {
        commands.insert_resource(Almanach {
            buildings: registrations.buildings.clone(),
            shards: registrations.shards.clone(),
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

    // === Shards ===

    pub fn get_shard_info(&self, shard_type: ShardType) -> &ShardInfo {
        self.shards.get(&shard_type)
            .expect(&format!("Shard {shard_type:?} not found in almanach"))
    }

    pub fn get_shard_info_mut(&mut self, shard_type: ShardType) -> &mut ShardInfo {
        self.shards.get_mut(&shard_type)
            .expect(&format!("Shard {shard_type:?} not found in almanach"))
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

#[derive(Clone)]
pub struct BuildingInfo {
    pub name: String,
    pub grid_imprint: GridImprint,
    pub cost: Vec<Cost>,
    pub baseline: HashMap<ModifierType, f32>,
    pub validate: PlacementValidatorFn,
    pub annotate: PlacementAnnotatorFn,
    pub sprite: Handle<Image>,
    pub top_sprite: Option<Handle<Image>>,
    pub placement: PlacementChannel,
}

impl From<&BuildingInfo> for ObjectPlacementInfo {
    fn from(info: &BuildingInfo) -> Self {
        Self {
            imprint: info.grid_imprint,
            validate: info.validate,
            annotate: info.annotate,
            placement: info.placement,
            preview_image: Some(info.sprite.clone()),
        }
    }
}

// ============================================================================
// WALL INFO
// ============================================================================

#[derive(Clone)]
pub struct WallInfo {
    pub name: String,
    pub grid_imprint: GridImprint,
    pub sprite: Handle<Image>,
    pub validate: PlacementValidatorFn,
    pub annotate: PlacementAnnotatorFn,
    pub placement: PlacementChannel,
}

impl From<&WallInfo> for ObjectPlacementInfo {
    fn from(info: &WallInfo) -> Self {
        Self {
            imprint: info.grid_imprint,
            validate: info.validate,
            annotate: info.annotate,
            placement: info.placement,
            preview_image: Some(info.sprite.clone()),
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
    pub sprites: Vec<Handle<Image>>,
    pub default_amount: u32,
    pub validate: PlacementValidatorFn,
    pub annotate: PlacementAnnotatorFn,
    pub placement: PlacementChannel,
}

impl From<&DarkOreInfo> for ObjectPlacementInfo {
    fn from(info: &DarkOreInfo) -> Self {
        Self {
            imprint: info.grid_imprint,
            validate: info.validate,
            annotate: info.annotate,
            placement: info.placement,
            preview_image: info.sprites.first().cloned(),
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
    pub annotate: PlacementAnnotatorFn,
    pub placement: PlacementChannel,
}

impl From<&QuantumFieldInfo> for ObjectPlacementInfo {
    fn from(info: &QuantumFieldInfo) -> Self {
        Self {
            imprint: info.default_imprint(),
            validate: info.validate,
            annotate: info.annotate,
            placement: info.placement,
            preview_image: None,
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
    pub annotate: PlacementAnnotatorFn,
    pub placement: PlacementChannel,
}

impl From<&WispInfo> for ObjectPlacementInfo {
    fn from(info: &WispInfo) -> Self {
        Self {
            imprint: info.grid_imprint,
            validate: info.validate,
            annotate: info.annotate,
            placement: info.placement,
            preview_image: None,
        }
    }
}

// ============================================================================
// SHARD INFO
// ============================================================================

/// The cost and forge duration required to craft one shard of a given type.
#[derive(Clone)]
pub struct ShardRecipe {
    pub cost: Vec<Cost>,
    pub duration: std::time::Duration,
}

/// Metadata for a shard type: display name, description, icon, and optional forge recipe.
///
/// A `None` recipe means this shard type cannot be forged and will not appear in the
/// forge's button list.
#[derive(Clone)]
pub struct ShardInfo {
    pub name: String,
    pub description: String,
    pub icon: Handle<Image>,
    pub recipe: Option<ShardRecipe>,
}
