pub mod model;
pub mod outcomes;
pub mod process_events;
pub mod persistence;

pub mod prelude {
    pub use crate::model::{
        ActiveResearch, CheckForObsoletion, Completed, Obsolete, OutcomeDisplay, OutcomeSatisfied,
        OutcomeSeed, Research, ResearchCardDisplay, ResearchCatalog, ResearchInstantiated,
        ResearchOutcomeOf, ResearchOutcomes, ResearchProgress, ResearchSpec, ResearchType,
    };
    pub use crate::outcomes::{FulfillOutcome, GrantShardBlueprint};
    pub use crate::process_events::{ResearchCompleted, SetActiveResearch, StopResearch};
}
