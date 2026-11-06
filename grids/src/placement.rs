use bevy::{platform::collections::HashSet, prelude::*};

use game_core::prelude::{EdgeSide, GridArea, GridCoords, GridImprint, MapObject};

/// Event emitted when the grid placer's state changes (area, style, etc).
/// Other systems can observe this to react to placer updates.
#[derive(Event, Clone, Copy, Debug)]
pub struct GridPlacerChanged;

/// Emitted when placement begins for an object.
///
/// Domain UI observes this event to create controls such as size or style selectors.
/// Changing the imprint, style, or direction does not start a new session.
#[derive(Event, Clone, Copy, Debug)]
pub struct BeginPlacing(pub MapObject);

/// Event emitted when the placer deactivates or switches to a different object type.
/// Domain UIs (e.g., QuantumField size selector) observe this to hide/cleanup.
#[derive(Event, Clone, Copy, Debug)]
pub struct StopPlacing;

/// Requests a property change within the active placement session.
#[derive(Event, Clone, Copy, Debug)]
pub enum GridPlacerOverridePropertyRequest {
    Imprint(GridImprint),
    /// Selects a variant from the object's domain; see [`PlacementStyle`].
    Style(u8),
    /// Sets or clears the object's direction; see [`PlacementDirection`].
    Direction(Option<EdgeSide>),
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

/// Variant selected for placement, interpreted by the object's domain.
/// The placer resets it to 0 when a session begins.
#[derive(Component, Clone, Copy, Default)]
pub struct PlacementStyle(pub u8);

/// Optional direction selected for placement, interpreted by the object's domain.
/// For walls, it selects the ramp's high side; `None` selects a flat wall.
/// The placer resets it to `None` when a session begins.
#[derive(Component, Clone, Copy, Default)]
pub struct PlacementDirection(pub Option<EdgeSide>);

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
