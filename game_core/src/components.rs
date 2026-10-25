use bevy::prelude::*;

use crate::grid::GridArea;

/// Component for entities that are bound to the map and shall be removed on its change
#[derive(Component, Default)]
pub struct MapBound;

/// Authored identity for a piece of content. Unique within its scope.
#[derive(Component, Clone, Debug, Default, Hash, PartialEq, Eq)]
pub struct ContentId(pub String);

impl From<String> for ContentId {
    fn from(s: String) -> Self { Self(s) }
}

impl From<&str> for ContentId {
    fn from(s: &str) -> Self { Self(s.to_string()) }
}

#[derive(Component, Clone, Copy)]
pub struct IntegrityPoints {
    pub current: f32,
    pub max: f32, // A helper, source of truth is in MaxIntegrityPoints component
}
impl IntegrityPoints {
    pub fn new(max: f32) -> Self {
        Self { current: max, max }
    }
    /// Sets the maximum and clamps the current points to it.
    pub fn set_max(&mut self, max: f32) {
        self.max = max;
        self.current = self.current.min(max);
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

// ============================================================================
// Power & Operational State
// ============================================================================

/// Entity uses power and should have its power state managed by the
/// energy-supply systems. Plain marker — power state is read via `IsPowered`.
#[derive(Component, Default)]
#[require(GridArea)]
pub struct NeedsPower;

/// Marker indicating the entity currently has power. Inserted/removed by
/// energy-supply systems in `game_core_internal`. Absence ⇒ no power.
#[derive(Component, Default)]
pub struct IsPowered;

/// Marker inserted when an entity is capable of doing its work.
/// External systems query this instead of reconstructing the condition
/// from primitive state components. Each entity owns the logic that sets it.
#[derive(Component, Default)]
pub struct IsOperational;

/// Player chose to disable this entity.
#[derive(Component, Default, Clone, Copy)]
pub struct DisabledByPlayer;
