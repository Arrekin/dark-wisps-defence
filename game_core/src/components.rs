use bevy::prelude::*;

// Component for entities that are bound to the map and shall be removed on its change
#[derive(Component, Default)]
pub struct MapBound;

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
