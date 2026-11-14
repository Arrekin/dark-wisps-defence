//! # Energy supply overlay
//!
//! The overlay draws the energy supply grid, or its preview while one exists. Callers that want the
//! overlay shown write the highlight they want into [`EnergySupplyOverlayConfig::secondary_mode`]
//! and clear it when they no longer want it. At most one caller wants the overlay at a time.

use bevy::prelude::*;

use game_core::prelude::GridArea;

/// The overlay shows while it is globally enabled or `secondary_mode` is set. When globally
/// enabled without a secondary mode, it highlights all supply; `secondary_mode` takes precedence.
#[derive(Resource, Default)]
pub struct EnergySupplyOverlayConfig {
    pub is_overlay_globally_enabled: bool,
    /// The highlight wanted by a caller, or `None` while nothing wants the overlay.
    pub secondary_mode: Option<EnergySupplyOverlaySecondaryMode>,
}

/// What the overlay highlights.
#[derive(Clone, Debug, PartialEq)]
pub enum EnergySupplyOverlaySecondaryMode {
    /// Every supply is highlighted.
    HighlightAll,
    /// Every supply is dimmed.
    HighlightNone,
    /// Only this supplier's coverage is highlighted.
    HighlightSupplier { supplier: Entity },
    /// Only the coverage of suppliers covering this area is highlighted.
    HighlightAreaSuppliers { area: GridArea },
}
