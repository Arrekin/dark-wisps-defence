use bevy::prelude::*;

pub mod expedition_drone;

/// Links a unit (e.g., ExpeditionDrone) to its owning building.
/// Used for: refueling location, operational checks (is home powered?), UI grouping.
#[derive(Component)]
#[relationship(relationship_target = HomeBaseLinkedObjects)]
pub struct HomeBase(pub Entity);

/// Inverse of HomeBase - auto-populated by Bevy's relationship system.
/// Query this on buildings to find all units that consider it home.
#[derive(Component)]
#[relationship_target(relationship = HomeBase)]
pub struct HomeBaseLinkedObjects(Vec<Entity>);

pub mod prelude {
    pub use super::{HomeBase, HomeBaseLinkedObjects};
}
