use crate::prelude::*;

/// Marks an entity as a valid target for expedition drones.
///
/// Drones accumulate scan progress here while their beam is active over this entity.
/// The owning system (e.g., QuantumField) should poll and consume progress via
/// `take_accumulated_scan_progress()` to drive its own mechanics.
///
/// This decoupling allows different entity types to use drone scanning without
/// the drone system needing to know about their specific progression logic.
#[derive(Component, Default)]
pub struct ExpeditionZone {
    pub accumulated_scan_progress: f32,
}
impl ExpeditionZone {
    /// Consumes and returns accumulated progress, resetting to zero.
    pub fn take_accumulated_scan_progress(&mut self) -> f32 {
        let progress = self.accumulated_scan_progress;
        self.accumulated_scan_progress = 0.0;
        progress
    }
}