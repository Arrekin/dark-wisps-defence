pub mod traits;
pub mod components;
pub mod display;
pub mod z_depth;
pub mod events;
pub mod math;
pub mod grid;
pub mod types;
pub mod motion;
pub mod moments;

pub mod prelude {
    // Foundation vocabulary shared across domain crates. Narrow utilities stay out — e.g.
    // `angle_difference` is imported explicitly via `game_core::math::angle_difference`.

    // Re-export the derive macros so `#[derive(SSS)]` / `#[derive(Property)]` /
    // `#[derive(MomentKind)]` work wherever the traits are in scope.
    pub use lib_derive::{FromEntity, MomentKind, Property, SSS};

    pub use crate::components::{ContentId, DisabledByPlayer, FieldAffectable, IntegrityPoints, IsOperational, IsPowered, MapBound, NeedsPower};
    pub use crate::display::{DisplayDescription, DisplayIcon, DisplayIconSwitcher, DisplayName, DisplayOrder};
    pub use crate::events::{DamageMessage, TechnicalChange, TechnicalStateChanged};
    pub use crate::grid::{CELL_SIZE, GridCoords, GridImprint, MapInfo};
    pub use crate::moments::{
        HasMoments, Moment, MomentHappened, MomentKind, MomentOf, MomentOfInterest, MomentWatchers,
    };
    pub use crate::traits::{Property, SSS};
    pub use crate::types::{BuildingType, MapObject, ShardType, TowerType, WispType};
    pub use crate::z_depth::*;          // ZDepth + associated layer constants
}
