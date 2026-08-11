pub mod components;
pub mod events;
pub mod research_bar;

pub mod prelude {
    pub use crate::components::{
        Research, ResearchActive, ResearchAvailable, ResearchCompleted, ResearchRuntime,
        ResearchState, ResearchUISelected,
    };
    pub use crate::events::{
        ResearchDisplayDataUpdated, ResearchFinished, ResearchUnitPaid, SeedResearches, SetActiveResearch, StopResearch,
    };
}
