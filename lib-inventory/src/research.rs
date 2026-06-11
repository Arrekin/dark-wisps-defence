//! # Research — data model, definitions, instantiation
//!
//! A research is a **per-map entity** (with outcome child entities) that the player advances over
//! time to permanently edit capabilities (typically granting blueprints). Definitions are static
//! defaults; instantiating one clones its data onto a fresh entity (so it is independently editable)
//! and spawns its default outcomes, then fires [`ResearchInstantiated`] so decoupled modifier
//! systems can compose further. Research itself knows nothing of those modifiers.
//!
//! Mechanics live in `research_process`, outcomes in `research_outcomes`, persistence in
//! `research_persistence`. Only the panel UI lives in the main crate.

use std::time::Duration;

use crate::lib_prelude::*;
use strum::{Display, EnumString, EnumIter, IntoEnumIterator};

use crate::research_outcomes::GrantShardBlueprint;
use crate::research_persistence::{self, BuilderResearch};
use crate::research_process;

pub mod research_prelude {
    pub use super::{
        Research, ResearchSpec, ResearchProgress, ActiveResearch, Completed, Obsolete,
        OutcomeSatisfied, ResearchOutcomeOf, ResearchOutcomes, OutcomeDisplay, ResearchCardDisplay,
        ResearchType, ResearchDefinition, ResearchCatalog, OutcomeSeed, ResearchInstantiated,
        CheckForObsoletion, instantiate,
    };
    pub use crate::research_outcomes::{GrantShardBlueprint, FulfillOutcome};
    pub use crate::research_process::{ResearchCompleted, SetActiveResearch, StopResearch};
}

pub struct ResearchPlugin;
impl Plugin for ResearchPlugin {
    fn build(&self, app: &mut App) {
        let asset_server = app.world().resource::<AssetServer>();
        let catalog = ResearchCatalog::build(asset_server);
        app
            .insert_resource(catalog)
            .add_observer(BuilderResearch::on_add)
            .add_observer(GrantShardBlueprint::on_add)
            .add_observer(GrantShardBlueprint::on_shard_blueprint_acquired)
            .add_observer(research_process::on_check_for_obsoletion)
            .add_observer(research_process::on_outcome_satisfied_inserted)
            .add_observer(research_process::on_outcome_satisfied_removed)
            .add_observer(research_process::on_set_active_research)
            .add_observer(research_process::on_stop_research)
            .add_systems(Update, research_process::research_tick.run_if(in_state(GameState::Running)))
            .add_systems(OnEnter(MapLoadingStage::Ready), seed_research)
            .register_db_loader::<BuilderResearch>(MapLoadingStage::SpawnMapElements)
            .register_db_loader::<research_persistence::ShardBlueprintOutcomeLoader>(MapLoadingStage::SpawnMapElements)
            .register_db_saver(research_persistence::save_researches)
            .register_db_saver(research_persistence::save_shard_blueprint_outcomes)
            ;
    }
}

// ============================================================================
// TAXONOMY
// ============================================================================

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Display, EnumString, EnumIter)]
pub enum ResearchType {
    FireShardRecipe,
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
    fn build(asset_server: &AssetServer) -> Self {
        let mut definitions = HashMap::new();
        definitions.insert(ResearchType::FireShardRecipe, ResearchDefinition {
            name: "Fire Shard Recipe".to_string(),
            description: "Unlocks the blueprint to forge Fire shards.".to_string(),
            icon: asset_server.load("ui/shards/shard_fire.png"),
            cost: vec![Cost { resource_type: ResourceType::Essence(EssenceType::Fire), amount: 100 }],
            duration: Duration::from_secs(30),
            default_outcomes: vec![OutcomeSeed::ShardBlueprint(ShardType::Fire)],
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

/// Spawns a fresh research instance. The actual construction — cloning the definition, spawning
/// default outcomes, and firing [`ResearchInstantiated`] — happens in `BuilderResearch::on_add`,
/// shared with the load path. Returns the research entity.
pub fn instantiate(commands: &mut Commands, research_type: ResearchType) -> Entity {
    commands.spawn(BuilderResearch::new(research_type)).id()
}

/// Instantiates any research not already present (fresh map). Loaded maps already have their research
/// entities, so this is a no-op for them. Mirrors `seed_starting_blueprints`.
fn seed_research(
    mut commands: Commands,
    existing: Query<&Research>,
) {
    let present: Vec<ResearchType> = existing.iter().map(|research| research.0).collect();
    for research_type in ResearchType::iter() {
        if !present.contains(&research_type) {
            instantiate(&mut commands, research_type);
        }
    }
}
