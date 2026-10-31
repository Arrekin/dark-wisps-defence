use bevy::{platform::collections::HashSet, prelude::*};

use game_core::prelude::{GridArea, GridCoords, GridImprint, MapObject};

/// Event emitted when the grid placer's state changes (area, style, etc).
/// Other systems can observe this to react to placer updates.
#[derive(Event, Clone, Copy, Debug)]
pub struct GridPlacerChanged;

/// Emitted when placement begins for an object.
///
/// Domain UI observes this event to create controls such as size or style selectors. Imprint and
/// style changes within the same session do not emit another event.
#[derive(Event, Clone, Copy, Debug)]
pub struct BeginPlacing(pub MapObject);

/// Event emitted when the placer deactivates or switches to a different object type.
/// Domain UIs (e.g., QuantumField size selector) observe this to hide/cleanup.
#[derive(Event, Clone, Copy, Debug)]
pub struct StopPlacing;

/// Event to request modification of the grid placer's state.
/// Placer observes this and handles changes internally.
#[derive(Event, Clone, Copy, Debug)]
pub enum GridPlacerOverridePropertyRequest {
    OverrideImprint(GridImprint),
    /// Index into whatever variant table the placed object's domain keeps.
    /// See [`PlacementStyle`].
    OverrideStyle(u32),
}

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

/// Which variant of the object is being placed. The placer carries the number and never
/// interprets it; the domain that owns the object decides what it selects. Reset when a
/// placement session begins, so a choice made for one object does not carry to the next.
#[derive(Component, Clone, Copy, Default)]
pub struct PlacementStyle(pub u32);

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
