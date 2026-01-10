use crate::prelude::*;

/// Component for map objects that can be scanned by expedition drones.
/// Progress accumulates while drone beams are active over this zone.
/// Places can consume the progress to forward their own progress.
#[derive(Component, Default)]
pub struct ExpeditionZone {
    pub accumulated_scan_progress: f32,
}
impl ExpeditionZone {
    pub fn take_accumulated_scan_progress(&mut self) -> f32 {
        let progress = self.accumulated_scan_progress;
        self.accumulated_scan_progress = 0.0;
        progress
    }
}