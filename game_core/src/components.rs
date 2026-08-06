use bevy::prelude::*;

use crate::grid::{GridCoords, GridImprint};

// Component for entities that are bound to the map and shall be removed on its change
#[derive(Component, Default)]
pub struct MapBound;

/// Authored identity for a piece of map content. Unique within a map; the
/// editor validates. Exists to match content and to be shown/edited — nothing
/// may branch on its value.
#[derive(Component, Clone, Debug, Default, Hash, PartialEq, Eq)]
pub struct ContentId(pub String);

impl From<String> for ContentId {
    fn from(s: String) -> Self { Self(s) }
}

impl From<&str> for ContentId {
    fn from(s: &str) -> Self { Self(s.to_string()) }
}

#[derive(Component)]
pub struct IntegrityPoints {
    pub current: f32,
    pub max: f32, // A helper, source of truth is in MaxIntegrityPoints component
}
impl IntegrityPoints {
    pub fn new(max: f32) -> Self {
        Self { current: max, max }
    }
    pub fn get_current(&self) -> f32 {
        self.current
    }
    pub fn get_max(&self) -> f32 {
        self.max
    }
    pub fn get_percent(&self) -> f32 {
        self.current / self.max
    }
    pub fn decrease(&mut self, amount: f32) {
        self.current = (self.current - amount).max(0.);
    }
    pub fn is_dead(&self) -> bool {
        self.current <= 0.
    }
}
impl Default for IntegrityPoints {
    fn default() -> Self {
        Self { current: f32::MAX, max: f32::MAX }
    }
}

/// Tracks which `ForceField` entity currently owns this entity's grid cell.
///
/// Managed exclusively by `field_tracking_system` in the game crate.
/// Added at spawn to any entity that can be affected by force fields (wisps, drones, etc.).
#[derive(Component, Default)]
pub struct FieldAffectable {
    pub current_field: Option<Entity>,
}

// ============================================================================
// Power & Operational State
// ============================================================================

/// Entity uses power and should have its power state managed by the
/// energy-supply systems. Plain marker — power state is read via `IsPowered`.
#[derive(Component, Default)]
#[require(GridCoords, GridImprint)]
pub struct NeedsPower;

/// Marker indicating the entity currently has power. Inserted/removed by
/// energy-supply systems in `grids_internal`. Absence ⇒ no power.
#[derive(Component, Default)]
pub struct IsPowered;

/// Marker inserted when an entity is capable of doing its work.
/// External systems query this instead of reconstructing the condition
/// from primitive state components. Each entity owns the logic that sets it.
#[derive(Component, Default)]
pub struct IsOperational;

/// Player chose to disable this entity.
#[derive(Component, Default)]
pub struct DisabledByPlayer;
