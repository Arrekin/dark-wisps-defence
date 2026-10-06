use bevy::{platform::collections::HashMap, prelude::*};

use alteration::modifiers::prelude::ModifierType;
use game_core::prelude::{BuildingType, ContentId, GridImprint, MapObject, Shard};
use grids::placement::{ObjectPlacementInfo, PlacementAnnotatorFn, PlacementModes, PlacementValidatorFn};
use resources::prelude::{ResourceAmount, ResourceType};
use shards::prelude::ShardSocket;
use states::prelude::MapLoadingStage;

pub mod prelude {
    pub use super::{Almanach, AlmanachAppExt};
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
    pub resources: HashMap<ResourceType, ResourceInfo>,
    pub shards: HashMap<Shard, ShardInfo>,
    pub researches: HashMap<ContentId, ResearchSpawnFn>,
    pub walls: Option<WallInfo>,
    pub dark_ore: Option<DarkOreInfo>,
    pub quantum_fields: Option<QuantumFieldInfo>,
    pub wisps: Option<WispInfo>,
}

pub trait AlmanachAppExt {
    fn register_building(&mut self, building_type: BuildingType, info: BuildingInfo) -> &mut Self;
    fn register_resource(&mut self, resource_type: ResourceType, info: ResourceInfo) -> &mut Self;
    /// Registers a shard both as a resource (its presentation) and as a shard (its recipe).
    fn register_shard(&mut self, shard: Shard, resource_info: ResourceInfo, shard_info: ShardInfo) -> &mut Self;
    fn register_research(&mut self, content_id: impl Into<ContentId>, spawn_fn: ResearchSpawnFn) -> &mut Self;
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

    fn register_resource(&mut self, resource_type: ResourceType, info: ResourceInfo) -> &mut Self {
        self.init_resource::<AlmanachRegistrations>();
        self.world_mut().resource_mut::<AlmanachRegistrations>()
            .resources.insert(resource_type, info);
        self
    }

    fn register_shard(&mut self, shard: Shard, resource_info: ResourceInfo, shard_info: ShardInfo) -> &mut Self {
        self.register_resource(ResourceType::Shard(shard), resource_info);
        self.world_mut().resource_mut::<AlmanachRegistrations>()
            .shards.insert(shard, shard_info);
        self
    }

    fn register_research(&mut self, content_id: impl Into<ContentId>, spawn_fn: ResearchSpawnFn) -> &mut Self {
        self.init_resource::<AlmanachRegistrations>();
        self.world_mut().resource_mut::<AlmanachRegistrations>()
            .researches.insert(content_id.into(), spawn_fn);
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

/// Spawn function for a research definition. Takes the `ContentId` to insert
/// on the entity. The editor calls this to seed or re-seed a research.
pub type ResearchSpawnFn = fn(&mut Commands, &ContentId);

#[derive(Clone, Copy)]
pub enum AccessPattern {
    Player,
    Admin,
}

/// Callback that spawns a tile tooltip anchored to the supplied entity.
pub type ObjectTooltipFn = fn(&mut Commands, Entity, MapObject);

/// Optional side-menu behavior for a map object.
#[derive(Clone)]
pub struct ObjectPresentation {
    /// No tooltip is spawned when this is `None`.
    pub tooltip: Option<ObjectTooltipFn>,
}

#[derive(Resource)]
pub struct Almanach {
    buildings: HashMap<BuildingType, BuildingInfo>,
    resources: HashMap<ResourceType, ResourceInfo>,
    shards: HashMap<Shard, ShardInfo>,
    pub researches: HashMap<ContentId, ResearchSpawnFn>,
    pub walls: WallInfo,
    pub dark_ore: DarkOreInfo,
    pub quantum_fields: QuantumFieldInfo,
    pub wisps: WispInfo,
}

impl Almanach {
    fn init_from_registrations(mut commands: Commands, registrations: Res<AlmanachRegistrations>) {
        commands.insert_resource(Almanach {
            buildings: registrations.buildings.clone(),
            resources: registrations.resources.clone(),
            shards: registrations.shards.clone(),
            researches: registrations.researches.clone(),
            walls: registrations.walls.clone().expect("WallInfo not registered in AlmanachRegistrations"),
            dark_ore: registrations.dark_ore.clone().expect("DarkOreInfo not registered in AlmanachRegistrations"),
            quantum_fields: registrations.quantum_fields.clone().expect("QuantumFieldInfo not registered in AlmanachRegistrations"),
            wisps: registrations.wisps.clone().expect("WispInfo not registered in AlmanachRegistrations"),
        });
    }

    // === Buildings ===

    pub fn get_building_info(&self, building_type: BuildingType) -> &BuildingInfo {
        self.buildings.get(&building_type)
            .unwrap_or_else(|| panic!("Building {building_type:?} not found in almanach"))
    }

    pub fn get_building_info_mut(&mut self, building_type: BuildingType) -> &mut BuildingInfo {
        self.buildings.get_mut(&building_type)
            .unwrap_or_else(|| panic!("Building {building_type:?} not found in almanach"))
    }

    /// Tower variants in deterministic menu order. Access does not affect the tower list.
    pub fn constructible_towers(&self, _access: AccessPattern) -> impl Iterator<Item = BuildingType> {
        BuildingType::all().filter(|building_type| matches!(building_type, BuildingType::Tower(_)))
    }

    /// Non-tower variants in deterministic menu order. The editor catalog also includes the
    /// Main Base.
    pub fn constructible_buildings(&self, access: AccessPattern) -> impl Iterator<Item = BuildingType> {
        BuildingType::all().filter(move |building_type| {
            !matches!(building_type, BuildingType::Tower(_))
                && (matches!(access, AccessPattern::Admin) || !matches!(building_type, BuildingType::MainBase))
        })
    }

    // === Resources ===

    pub fn get_resource_info(&self, resource_type: impl Into<ResourceType>) -> &ResourceInfo {
        let resource_type = resource_type.into();
        self.resources.get(&resource_type)
            .unwrap_or_else(|| panic!("Resource {resource_type:?} not found in almanach"))
    }

    // === Shards ===

    pub fn get_shard_info(&self, shard: Shard) -> &ShardInfo {
        self.shards.get(&shard)
            .unwrap_or_else(|| panic!("Shard {shard:?} not found in almanach"))
    }

    pub fn get_shard_info_mut(&mut self, shard: Shard) -> &mut ShardInfo {
        self.shards.get_mut(&shard)
            .unwrap_or_else(|| panic!("Shard {shard:?} not found in almanach"))
    }

    // === Map objects ===

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

    /// Returns the side-menu presentation registered for a map object.
    pub fn presentation_for(&self, map_object: MapObject) -> &ObjectPresentation {
        match map_object {
            MapObject::Building(building_type) => &self.get_building_info(building_type).presentation,
            MapObject::Wall => &self.walls.presentation,
            MapObject::DarkOre => &self.dark_ore.presentation,
            MapObject::QuantumField => &self.quantum_fields.presentation,
            MapObject::Wisp(_) => &self.wisps.presentation,
        }
    }
}

// ============================================================================
// BUILDING INFO
// ============================================================================

#[derive(Clone)]
pub struct BuildingInfo {
    pub name: String,
    pub description: String,
    pub grid_imprint: GridImprint,
    pub cost: Vec<ResourceAmount>,
    pub baseline: HashMap<ModifierType, f32>,
    /// Default sockets with a `ContentId` unique within the building. Empty if it takes no shards.
    pub sockets: Vec<(ContentId, ShardSocket)>,
    pub validate: PlacementValidatorFn,
    pub annotate: PlacementAnnotatorFn,
    pub sprite: Handle<Image>,
    pub top_sprite: Option<Handle<Image>>,
    pub placement: PlacementModes,
    pub presentation: ObjectPresentation,
}

impl From<&BuildingInfo> for ObjectPlacementInfo {
    fn from(info: &BuildingInfo) -> Self {
        Self {
            imprint: info.grid_imprint,
            validate: info.validate,
            annotate: info.annotate,
            placement: info.placement,
        }
    }
}

// ============================================================================
// WALL INFO
// ============================================================================

#[derive(Clone)]
pub struct WallInfo {
    pub name: String,
    pub description: String,
    pub grid_imprint: GridImprint,
    pub validate: PlacementValidatorFn,
    pub annotate: PlacementAnnotatorFn,
    pub placement: PlacementModes,
    pub presentation: ObjectPresentation,
}

impl From<&WallInfo> for ObjectPlacementInfo {
    fn from(info: &WallInfo) -> Self {
        Self {
            imprint: info.grid_imprint,
            validate: info.validate,
            annotate: info.annotate,
            placement: info.placement,
        }
    }
}

// ============================================================================
// DARK ORE INFO
// ============================================================================

#[derive(Clone)]
pub struct DarkOreInfo {
    pub name: String,
    pub description: String,
    pub grid_imprint: GridImprint,
    pub max_field_saturation: u32,
    pub validate: PlacementValidatorFn,
    pub annotate: PlacementAnnotatorFn,
    pub placement: PlacementModes,
    pub presentation: ObjectPresentation,
}

impl From<&DarkOreInfo> for ObjectPlacementInfo {
    fn from(info: &DarkOreInfo) -> Self {
        Self {
            imprint: info.grid_imprint,
            validate: info.validate,
            annotate: info.annotate,
            placement: info.placement,
        }
    }
}

// ============================================================================
// QUANTUM FIELD INFO
// ============================================================================

#[derive(Clone)]
pub struct QuantumFieldInfo {
    pub name: String,
    pub description: String,
    pub min_size: i32,
    pub max_size: i32,
    pub default_size: i32,
    pub validate: PlacementValidatorFn,
    pub annotate: PlacementAnnotatorFn,
    pub placement: PlacementModes,
    pub presentation: ObjectPresentation,
}

impl From<&QuantumFieldInfo> for ObjectPlacementInfo {
    fn from(info: &QuantumFieldInfo) -> Self {
        Self {
            imprint: info.default_imprint(),
            validate: info.validate,
            annotate: info.annotate,
            placement: info.placement,
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
    /// Shared by all wisp types; each display name comes from its `WispType`.
    pub description: String,
    pub grid_imprint: GridImprint,
    pub validate: PlacementValidatorFn,
    pub annotate: PlacementAnnotatorFn,
    pub placement: PlacementModes,
    pub presentation: ObjectPresentation,
}

impl From<&WispInfo> for ObjectPlacementInfo {
    fn from(info: &WispInfo) -> Self {
        Self {
            imprint: info.grid_imprint,
            validate: info.validate,
            annotate: info.annotate,
            placement: info.placement,
        }
    }
}

// ============================================================================
// RESOURCE INFO
// ============================================================================

/// How a resource is presented to the player wherever it appears: cost chips, pickers, panels.
#[derive(Clone)]
pub struct ResourceInfo {
    pub name: String,
    pub description: String,
    pub icon: Handle<Image>,
}

// ============================================================================
// SHARD INFO
// ============================================================================

/// The cost and forge duration required to craft one shard.
#[derive(Clone)]
pub struct ShardRecipe {
    pub cost: Vec<ResourceAmount>,
    pub duration: std::time::Duration,
}

/// Shard-specific metadata for one shard (type and tier). Its name, description and icon are its
/// [`ResourceInfo`].
///
/// A `None` recipe means this shard cannot be forged and will not appear in the forge's button list.
#[derive(Clone)]
pub struct ShardInfo {
    pub recipe: Option<ShardRecipe>,
}
