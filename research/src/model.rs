use std::time::Duration;

use bevy::{
    platform::collections::HashMap,
    prelude::*,
};
use strum::{Display, EnumIter, EnumString};

use game_core::prelude::{ShardType, SSS};
use resources::prelude::{Cost, EssenceType, ResourceType};

// ============================================================================
// TAXONOMY
// ============================================================================

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Display, EnumString, EnumIter)]
pub enum ResearchType {
    FireShardRecipe,
    WaterShardRecipe,
    LightShardRecipe,
    ElectricShardRecipe,
}

// ============================================================================
// INSTANCE COMPONENTS (saved per-map entities)
// ============================================================================

/// Identity of a research instance entity, linking it to its taxonomy.
#[derive(Component)]
pub struct Research(pub ResearchType);

/// Cloned, per-instance tunable gameplay data. `cost` is the total, drained over `duration` as the
/// research progresses (pay-as-you-go).
#[derive(Component)]
pub struct ResearchSpec {
    pub cost: Vec<Cost>,
    pub duration: Duration,
}

/// Present only while a research is in-flight (started, not yet finished). `fraction` is 0..1 and
/// determines how much of the cost has been paid (`paid = floor(fraction * cost)`).
#[derive(Component, Default)]
pub struct ResearchProgress {
    pub fraction: f32,
}

/// Marker on the single research currently receiving resource flow. At most one exists.
#[derive(Component)]
pub struct ActiveResearch;

/// Marker on a finished research. Permanent and saved; independent of blueprint possession.
#[derive(Component)]
pub struct Completed;

/// Marker on a research whose outputs are all already owned, so it is no longer actionable. It is the
/// generic aggregate of its outcomes' [`OutcomeSatisfied`] state — set/removed only by the
/// `CheckForObsoletion` handler. Not persisted: it re-derives through outcome reactions on load.
#[derive(Component)]
pub struct Obsolete;

/// Marker on an outcome entity meaning "what I grant is already owned." Each outcome KIND maintains
/// its own (set at spawn if owned, updated reactively from that kind's possession events). This is
/// the uniform signal the generic obsolescence aggregation reads — no kind knowledge leaks upward.
#[derive(Component)]
pub struct OutcomeSatisfied;

/// Links an outcome entity to its research. The research side is [`ResearchOutcomes`].
#[derive(Component)]
#[relationship(relationship_target = ResearchOutcomes)]
pub struct ResearchOutcomeOf(pub Entity);

/// All outcome entities of a research, populated automatically via [`ResearchOutcomeOf`].
#[derive(Component)]
#[relationship_target(relationship = ResearchOutcomeOf)]
pub struct ResearchOutcomes(Vec<Entity>);

/// Static display projection for an outcome entity, derived from its kind when spawned. The panel
/// renders outcomes from this without knowing their concrete type.
#[derive(Component)]
pub struct OutcomeDisplay {
    pub icon: Handle<Image>,
    pub title: String,
}

/// Static presentation projection for a research entity (definition-derived: title + icon). The
/// panel renders these; dynamic state (active / completed / obsolete / progress) is read via markers
/// and `ResearchProgress`, updated reactively rather than polled.
#[derive(Component)]
pub struct ResearchCardDisplay {
    pub title: String,
    pub icon: Handle<Image>,
}

// ============================================================================
// STATIC DEFINITIONS
// ============================================================================

/// A default outcome to spawn when a research is instantiated. Static authoring only — never
/// persisted (the spawned outcome entities are what persist, each in its own lane).
#[derive(Clone, Copy)]
pub enum OutcomeSeed {
    ShardBlueprint(ShardType),
}

/// Static defaults for a research type, cloned onto an instance at instantiation.
#[derive(Clone)]
pub struct ResearchDefinition {
    pub name: String,
    pub description: String,
    pub icon: Handle<Image>,
    pub cost: Vec<Cost>,
    pub duration: Duration,
    pub default_outcomes: Vec<OutcomeSeed>,
}

#[derive(Resource)]
pub struct ResearchCatalog {
    definitions: HashMap<ResearchType, ResearchDefinition>,
}
impl ResearchCatalog {
    pub fn build(asset_server: &AssetServer) -> Self {
        let mut definitions = HashMap::new();
        definitions.insert(ResearchType::FireShardRecipe, ResearchDefinition {
            name: "Fire Shard Recipe".to_string(),
            description: "Unlocks the blueprint to forge Fire shards.".to_string(),
            icon: asset_server.load("ui/shards/shard_fire.png"),
            cost: vec![Cost { resource_type: ResourceType::Essence(EssenceType::Fire), amount: 100 }],
            duration: Duration::from_secs(30),
            default_outcomes: vec![OutcomeSeed::ShardBlueprint(ShardType::Fire)],
        });
        definitions.insert(ResearchType::WaterShardRecipe, ResearchDefinition {
            name: "Water Shard Recipe".to_string(),
            description: "Unlocks the blueprint to forge Water shards.".to_string(),
            icon: asset_server.load("ui/shards/shard_water.png"),
            cost: vec![Cost { resource_type: ResourceType::Essence(EssenceType::Water), amount: 100 }],
            duration: Duration::from_secs(30),
            default_outcomes: vec![OutcomeSeed::ShardBlueprint(ShardType::Water)],
        });
        definitions.insert(ResearchType::LightShardRecipe, ResearchDefinition {
            name: "Light Shard Recipe".to_string(),
            description: "Unlocks the blueprint to forge Light shards.".to_string(),
            icon: asset_server.load("ui/shards/shard_light.png"),
            cost: vec![Cost { resource_type: ResourceType::Essence(EssenceType::Light), amount: 100 }],
            duration: Duration::from_secs(30),
            default_outcomes: vec![OutcomeSeed::ShardBlueprint(ShardType::Light)],
        });
        definitions.insert(ResearchType::ElectricShardRecipe, ResearchDefinition {
            name: "Electric Shard Recipe".to_string(),
            description: "Unlocks the blueprint to forge Electric shards.".to_string(),
            icon: asset_server.load("ui/shards/shard_electric.png"),
            cost: vec![Cost { resource_type: ResourceType::Essence(EssenceType::Electric), amount: 100 }],
            duration: Duration::from_secs(30),
            default_outcomes: vec![OutcomeSeed::ShardBlueprint(ShardType::Electric)],
        });
        Self { definitions }
    }

    pub fn get(&self, research_type: ResearchType) -> &ResearchDefinition {
        self.definitions.get(&research_type)
            .unwrap_or_else(|| panic!("Research {research_type:?} not found in catalog"))
    }
}

// ============================================================================
// INSTANTIATION
// ============================================================================

/// Triggered on a freshly instantiated research entity. Modifier systems may observe it to compose
/// additional outcomes/components. Not fired on load (the saved composition is authoritative).
#[derive(EntityEvent, Clone, Copy)]
pub struct ResearchInstantiated {
    #[event_target]
    pub research: Entity,
    pub research_type: ResearchType,
}

/// Requests a (re-)evaluation of a research's obsolescence. Fired whenever an outcome's satisfaction
/// changes (born, acquired, or — later — revoked). The handler is generic: it only reads
/// [`OutcomeSatisfied`] markers, never outcome kinds.
#[derive(EntityEvent, Clone, Copy)]
pub struct CheckForObsoletion {
    #[event_target]
    pub research: Entity,
}

// ============================================================================
// BUILDER
// ============================================================================

/// Builds a research instance for both fresh spawns and loads. Fresh spawns
/// clone the definition, spawn default outcomes, and fire `ResearchInstantiated`; loads restore the
/// saved scalars and never re-fire (the saved composition, including modifier-added outcomes, is
/// authoritative).
#[derive(Component, SSS)]
pub struct BuilderResearch {
    pub research_type: ResearchType,
    /// Saved duration in seconds. `None` ⇒ fresh spawn (use catalog definition);
    /// `Some` ⇒ override with saved value (restore).
    pub duration_secs: Option<f32>,
    /// Saved cost. `None` ⇒ fresh spawn (use catalog definition); `Some` ⇒ override with saved value.
    pub cost: Option<Vec<Cost>>,
    /// `Some(fraction)` while in flight; `None` when not started or completed.
    pub progress: Option<f32>,
    /// Whether the player set this research active. False on fresh spawn.
    pub is_active: bool,
    /// Whether the research was completed. False on fresh spawn.
    pub is_completed: bool,
}
impl BuilderResearch {
    pub fn new(research_type: ResearchType) -> Self {
        Self {
            research_type,
            duration_secs: None,
            cost: None,
            progress: None,
            is_active: false,
            is_completed: false,
        }
    }
    pub fn with_duration_secs(mut self, duration_secs: f32) -> Self {
        self.duration_secs = Some(duration_secs);
        self
    }
    pub fn with_cost(mut self, cost: Vec<Cost>) -> Self {
        self.cost = Some(cost);
        self
    }
    pub fn with_progress(mut self, progress: f32) -> Self {
        self.progress = Some(progress);
        self
    }
    pub fn with_active(mut self) -> Self {
        self.is_active = true;
        self
    }
    pub fn with_completed(mut self) -> Self {
        self.is_completed = true;
        self
    }
}
