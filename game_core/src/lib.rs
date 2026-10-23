pub mod components;
pub mod display;
pub mod events;
pub mod grid;
pub mod math;
pub mod moments;
pub mod motion;
pub mod request_flag;
pub mod response_request;
pub mod traits;
pub mod types;
pub mod z_depth;

pub mod prelude {
    // Foundation vocabulary shared across domain crates. Narrow utilities stay out — e.g.
    // `angle_difference` is imported explicitly via `game_core::math::angle_difference`.

    // Re-export the derive macros so `#[derive(SSS)]` / `#[derive(Property)]` /
    // `#[derive(MomentKind)]` work wherever the traits are in scope.
    pub use lib_derive::{FromEntity, MomentKind, Property, SSS};

    pub use crate::components::{ContentId, DisabledByPlayer, IntegrityPoints, IsOperational, IsPowered, MapBound, NeedsPower};
    pub use crate::display::{DisplayDescription, DisplayIcon, DisplayIconSwitcher, DisplayName, DisplayOrder, FaceSurface, GHOST_ALPHA, MapObjectTooltipRequest, ObjectFaceRequest};
    pub use crate::events::{DamageMessage, TechnicalChange, TechnicalStateChanged};
    pub use crate::grid::{ALL_DIRECTIONS, AutoGridSnap, Bounds, CARDINAL_DIRECTIONS, CELL_SIZE, GridCoords, GridImprint, MapInfo};
    pub use crate::moments::{
        HasMoments, Moment, MomentHappened, MomentKind, MomentOf, MomentOfInterest, MomentWatchers,
    };
    pub use crate::response_request::ResponseRequest;
    pub use crate::traits::{OptionalCommands, Property, SSS};
    pub use crate::types::{BuildingType, MapObject, Shard, ShardTier, ShardType, TowerType, WispType};
    pub use crate::z_depth::ZDepth;
}
