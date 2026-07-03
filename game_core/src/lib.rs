pub mod traits;
pub mod components;
pub mod z_depth;
pub mod events;
pub mod math;
pub mod grid;
pub mod types;
pub mod motion;

pub mod prelude {
    // Foundation vocabulary shared across domain crates. Narrow utilities stay out — e.g.
    // `angle_difference` is imported explicitly via `game_core::math::angle_difference`.

    // Re-export the derive macros so `#[derive(SSS)]` / `#[derive(Property)]` work wherever the
    // traits are in scope.
    pub use lib_derive::{Property, SSS};

    pub use crate::components::{FieldAffectable, IntegrityPoints, MapBound};
    pub use crate::events::{DamageMessage, DynamicGameEvent};
    pub use crate::grid::{CELL_SIZE, GridCoords, GridImprint};
    pub use crate::motion::Locomotion;
    pub use crate::traits::{Property, SSS};
    pub use crate::types::{BuildingType, MapObject, ShardType, TowerType, WispType};
    pub use crate::z_depth::*;          // ZDepth + Z_* constants
}
