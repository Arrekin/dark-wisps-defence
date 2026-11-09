use bevy::{platform::collections::HashSet, prelude::*};

use game_core::prelude::{GridArea, GridCoords, GridImprint, MapObject};

/// Requests updated placement validity and cell highlights after the placer's area or object changes.
#[derive(Event, Clone, Copy, Debug)]
pub struct GridPlacerChanged;

/// Emitted when placement begins for an object.
///
/// Domain observers initialize parameters on the placer's session entity and create controls such
/// as size or style selectors. These parameters are available when the placer builds the ghost
/// after this event. Changing a parameter does not start a new session.
#[derive(Event, Clone, Copy, Debug)]
pub struct BeginPlacing(pub MapObject);

/// Event emitted when the placer deactivates or switches to a different object type.
/// Domain UIs (e.g., QuantumField size selector) observe this to hide/cleanup.
#[derive(Event, Clone, Copy, Debug)]
pub struct StopPlacing;

/// Overrides a placement setting for the active session, then rebuilds the ghost and revalidates.
#[derive(Event, Clone, Copy, Debug)]
pub enum GridPlacerOverridePropertyRequest {
    Imprint(GridImprint),
}

/// Requests a new ghost preview after domain parameters change on the placer's session entity.
/// Does not refresh placement validity or cell highlights.
#[derive(Event, Clone, Copy, Debug)]
pub struct GridPlacerGhostRebuildRequest;

/// Requests placement of the active object at the placer's current position.
///
/// Domain observers match the object variant:
///
/// ```ignore
/// let PlaceRequest(MapObject::Wall) = *trigger else { return };
/// ```
#[derive(Event, Clone, Copy, Debug)]
pub struct PlaceRequest(pub MapObject);

/// Requests removal at the placer's current position.
///
/// Observers match the object as for [`PlaceRequest`]. Objects without a matching observer do not
/// support removal.
#[derive(Event, Clone, Copy, Debug)]
pub struct RemoveRequest(pub MapObject);

/// When the placer emits place and remove requests: on press for burst and drag placement,
/// on release for a single click.
#[derive(Clone, Copy, Debug, Default)]
pub struct PlacementModes {
    pub place: PlacementMode,
    pub remove: PlacementMode,
}

impl PlacementModes {
    /// Both actions emit while the button is held, for objects placed in a stroke.
    pub fn on_press() -> Self {
        Self { place: PlacementMode::OnPress, remove: PlacementMode::OnPress }
    }
}

/// Placement validity state returned by validators. Discriminants are the `VALIDITY_*` constants
/// in `assets/shaders/grids/grid_placer.wesl`; the two must stay in step.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u32)]
pub enum PlacementValidity {
    Valid = 0,
    ValidUnpowered = 1,
    Invalid = 2,
}
impl PlacementValidity {
    pub fn gpu_index(self) -> u32 {
        self as u32
    }
}

/// Which kind of highlight a special cell carries.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CellHighlight {
    /// Blocks placement.
    Negative,
    /// Contributes positively (e.g. ore in range).
    Positive,
}

/// Whether placement triggers on press (burst mode) or release (single click).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PlacementMode {
    /// Emit on mouse button release (default, single placement per click)
    #[default]
    OnRelease,
    /// Emit while mouse button is held (burst mode for walls, etc.)
    OnPress,
}

/// Cells reserved for deferred placement, before the obstacle grid reflects the new objects.
///
/// Placement handlers check and reserve cells to prevent overlapping requests in the same frame.
/// Reservations are cleared in the `First` schedule of the next frame.
#[derive(Resource, Default)]
pub struct ReservedCoords {
    pub for_structures: HashSet<GridCoords>,
}
impl ReservedCoords {
    pub fn reserve(&mut self, area: GridArea) {
        self.for_structures.extend(area);
    }
    pub fn any_reserved(&self, area: GridArea) -> bool {
        area.iter().any(|cell| self.for_structures.contains(&cell))
    }
}
