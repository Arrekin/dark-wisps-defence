#![feature(adt_const_params)]

//! # Indexes
//!
//! Spatial indexes for structures, wisps, floor cover, field and tower coverage, emissions, and
//! energy supply. These indexes store domain types and support queries against the map.
//! Reusable grid storage and shared placement types live in `grids`.

pub mod emissions;
pub mod energy_supply;
pub mod fields;
pub mod floor_cover;
pub mod obstacles;
pub mod search;
pub mod tower_ranges;
pub mod wisps;

pub mod prelude {
    pub use super::obstacles::{ObstacleCell, ObstacleGrid};
}
