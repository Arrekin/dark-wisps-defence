//! # Placement contract
//!
//! A placement session stores the selected object, target area, and object-specific settings.
//! The placement tool sends [`PlaceRequest`] and [`RemoveRequest`] on player input. Each object's
//! domain owns the rules for accepting and carrying out those requests.
//!
//! Domains publish [`PlacementFeedback`] on the session for the tool to display. Feedback does
//! not gate requests: handlers validate against current state when a request arrives.

use bevy::{platform::collections::HashSet, prelude::*};

use game_core::prelude::{GridArea, GridCoords, GridPosition, MapObject};

/// Marks a placement session.
///
/// An active session carries [`PlacementObject`], set by the placement tool, with its required
/// [`PlacementModes`]. On [`BeginPlacing`] the object's domain inserts the target [`GridArea`],
/// other [`PlacementModes`] where the object needs them, and its own settings. The domain's
/// feedback systems keep [`PlacementFeedback`] current on it.
#[derive(Component, Default)]
pub struct PlacementSession;

/// The object selected for placement.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
#[require(PlacementModes)]
pub struct PlacementObject(pub MapObject);

/// Enables replacement of existing objects of the same kind during placement.
#[derive(Component, Clone, Copy, Debug, Default)]
pub struct ReplaceExisting;

/// Display feedback for the session's current placement.
///
/// Domains update this immutable component with `insert_if_neq`, allowing the tool to redraw
/// on `On<Insert<PlacementFeedback>>`. Request handlers must validate again before placing.
#[derive(Component, Clone, Debug, PartialEq)]
#[component(immutable)]
pub struct PlacementFeedback {
    pub validity: PlacementValidity,
    /// Whether the player can pay for placement, including free placement.
    pub is_affordable: bool,
    pub highlights: Vec<(GridCoords, CellHighlight)>,
}
impl PlacementFeedback {
    /// Creates feedback with affordability set to `true`; use `with_affordability` for paid placement.
    pub fn new(validity: PlacementValidity, highlights: Vec<(GridCoords, CellHighlight)>) -> Self {
        Self { validity, is_affordable: true, highlights }
    }

    pub fn with_validity(self, validity: PlacementValidity) -> Self {
        Self { validity, ..self }
    }

    pub fn with_highlights(self, highlights: Vec<(GridCoords, CellHighlight)>) -> Self {
        Self { highlights, ..self }
    }

    pub fn with_affordability(self, is_affordable: bool) -> Self {
        Self { is_affordable, ..self }
    }

    /// Returns `Valid` if every cell passes `accepts` and no cell is reserved; otherwise `Invalid`.
    ///
    /// Cells that fail `accepts` receive `Negative` highlights. Reservations affect validity only.
    pub fn every_cell(area: GridArea, reserved_coords: &ReservedCoords, accepts: impl Fn(GridCoords) -> bool) -> Self {
        let highlights: Vec<_> = area.iter()
            .filter(|coords| !accepts(*coords))
            .map(|coords| (coords, CellHighlight::Negative))
            .collect();
        let validity = if highlights.is_empty() && !reserved_coords.any_reserved(area) {
            PlacementValidity::Valid
        } else {
            PlacementValidity::Invalid
        };
        Self::new(validity, highlights)
    }

    /// Changes `Valid` to `Replaces` if any cell passes `holds_content`, replacing the highlights
    /// with `Removed` marks for those cells. Leaves feedback unchanged otherwise.
    pub fn replacing(self, area: GridArea, holds_content: impl Fn(GridCoords) -> bool) -> Self {
        if self.validity != PlacementValidity::Valid { return self; }
        let removed: Vec<_> = area.iter()
            .filter(|coords| holds_content(*coords))
            .map(|coords| (coords, CellHighlight::Removed))
            .collect();
        if removed.is_empty() { self } else { self.with_validity(PlacementValidity::Replaces).with_highlights(removed) }
    }
}

/// Whether the site supports placement, independent of affordability.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlacementValidity {
    Valid,
    /// Placement is allowed, but the object will have no power.
    ValidUnpowered,
    /// Placement is allowed and replaces existing content of the same kind.
    Replaces,
    Invalid,
}

/// Why a cell is highlighted in the placement preview.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CellHighlight {
    /// Blocks placement.
    Negative,
    /// Meets a placement requirement, such as ore beneath a mining complex.
    Positive,
    /// Contains content that placement will replace.
    Removed,
}

/// Systems that keep [`PlacementFeedback`] current on sessions.
#[derive(SystemSet, Clone, Copy, Eq, PartialEq, Hash, Debug)]
pub struct PlacementFeedbackSystems;

/// Requests placement of the selected object in the session's target area.
///
/// Domain observers match the object variant, read the area and settings from the session,
/// and validate against current state:
///
/// ```ignore
/// let PlaceRequest(MapObject::Wall) = *trigger else { return };
/// ```
#[derive(Event, Clone, Copy, Debug)]
pub struct PlaceRequest(pub MapObject);

/// Requests removal of the specified object kind at the session's target area.
///
/// Observers match the object as for [`PlaceRequest`]. Objects without a matching observer do not
/// support removal.
#[derive(Event, Clone, Copy, Debug)]
pub struct RemoveRequest(pub MapObject);

/// Cells claimed by placement handlers while spawns are waiting to update the obstacle grid.
///
/// Placement handlers check and reserve cells to prevent overlapping requests in the same frame.
/// Reservations are cleared in the `First` schedule of the next frame.
#[derive(Resource, Default)]
pub struct ReservedCoords(HashSet<GridCoords>);
impl ReservedCoords {
    pub fn reserve(&mut self, area: GridArea) {
        self.0.extend(area);
    }
    pub fn any_reserved(&self, area: GridArea) -> bool {
        area.iter().any(|cell| self.0.contains(&cell))
    }
    pub fn clear(&mut self) {
        self.0.clear();
    }
}

// ============================================================================
// PLACER TOOL
// ============================================================================

/// Mouse input behavior for placement and removal of the session's object.
#[derive(Component, Clone, Copy, Debug, Default)]
pub struct PlacementModes {
    pub place: PlacementMode,
    pub remove: PlacementMode,
}

impl PlacementModes {
    /// Enables placement and removal while dragging with a mouse button held.
    pub fn on_press() -> Self {
        Self { place: PlacementMode::OnPress, remove: PlacementMode::OnPress }
    }
}

/// When mouse input sends a placement or removal request.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PlacementMode {
    /// Sends one request when the mouse button is released.
    #[default]
    OnRelease,
    /// Sends requests while the mouse button is held.
    /// Placement sends at most one request per origin cell per press of the button; removal repeats
    /// each frame.
    OnPress,
}

/// Emitted when the placer's session changes its object or its area.
#[derive(Event, Clone, Copy, Debug)]
pub struct GridPlacerChanged;

/// Emitted when placement begins for an object. The session then holds only
/// [`PlacementSession`], [`PlacementObject`] and the default [`PlacementModes`].
///
/// The object's domain inserts on the session everything its placement needs: at least the
/// [`GridArea`] at `origin`, other [`PlacementModes`] when the default does not suit it, and its
/// own settings. It also creates controls such as size or style selectors. The placer builds the
/// ghost after this event, so the ghost can read the session. Changing a setting does not start a
/// new session.
///
/// ```ignore
/// let BeginPlacing { object: MapObject::Wall, origin } = *trigger else { return };
/// ```
#[derive(Event, Clone, Copy, Debug)]
pub struct BeginPlacing {
    pub object: MapObject,
    pub origin: GridPosition,
}

/// Emitted when the placement session ends, including when another object is selected.
#[derive(Event, Clone, Copy, Debug)]
pub struct StopPlacing;

/// Marks a domain's placement controls. The placer despawns them when the session ends.
#[derive(Component, Clone, Copy, Debug, Default)]
pub struct PlacementControls;

/// Requests a new ghost after a setting the ghost shows changes on the session.
#[derive(Event, Clone, Copy, Debug)]
pub struct GridPlacerGhostRebuildRequest;
