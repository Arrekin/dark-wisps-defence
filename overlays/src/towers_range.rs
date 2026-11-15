//! # Tower range overlay
//!
//! The overlay draws the ranges of all towers. Callers that want the overlay shown write the range
//! they want highlighted into [`TowersRangeOverlayConfig::secondary_mode`] and clear it when they
//! no longer want it. At most one caller wants the overlay at a time.

use bevy::prelude::*;

use game_core::prelude::AreaRange;

/// The overlay shows while it is globally enabled or `secondary_mode` is set. When globally
/// enabled without a secondary mode, no range is highlighted.
#[derive(Resource, Default)]
pub struct TowersRangeOverlayConfig {
    pub is_overlay_globally_enabled: bool,
    /// The highlight wanted by a caller, or `None` while nothing wants the overlay.
    pub secondary_mode: Option<TowersRangeOverlaySecondaryMode>,
}

/// What the overlay highlights.
#[derive(Clone, Debug, PartialEq)]
pub enum TowersRangeOverlaySecondaryMode {
    /// This tower's indexed range is highlighted, following its changes.
    HighlightTower { tower: Entity },
    /// This range is highlighted.
    HighlightRange { range: AreaRange },
}
