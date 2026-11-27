//! # Placement contract
//!
//! A [`PlacementSession`] entity holds the selected [`PlacementObject`], target [`GridArea`], and
//! any object-specific settings. Lifecycle and action events ([`BeginPlacing`], [`StopPlacing`],
//! [`PlaceRequest`], [`RemoveRequest`]) target a session entity, allowing multiple independent
//! sessions to coexist. Each object's domain validates and executes requests on its sessions and
//! reports outcomes via [`PlaceReport`] and [`RemoveReport`].
//!
//! [`UiPlacementSession`] marks the player's persistent session, driven by the placer tool. Only
//! it carries [`PlacementFeedback`] for visual preview. Feedback never gates requests: handlers
//! always re-validate against live world state when a request arrives.

use bevy::{platform::collections::HashSet, prelude::*};
use serde::Serialize;

use game_core::prelude::{GridArea, GridCoords, GridPosition, MapObject, ResponseRequest};

/// Marks a placement session entity.
///
/// An active session holds [`PlacementObject`] (which requires default [`PlacementModes`]), along
/// with the target [`GridArea`] and any object-specific settings inserted by the domain on
/// [`BeginPlacing`].
#[derive(Component, Default)]
pub struct PlacementSession;
impl PlacementSession {
    /// Spawns a standalone session for `object` and triggers [`BeginPlacing`] at `origin`.
    /// Trigger [`StopPlacing`] on the returned entity when done to despawn it.
    pub fn begin(commands: &mut Commands, object: MapObject, origin: GridPosition) -> Entity {
        let placement_session = commands.spawn((PlacementSession, PlacementObject(object))).id();
        commands.trigger(BeginPlacing::new(placement_session, origin));
        placement_session
    }
}

/// Marks the singleton placement session driven by the player's UI placer tool.
///
/// Carries [`PlacementFeedback`] and drives ghost previews, overlay highlights, and
/// [`PlacementControls`]. Queryable via `Single<.., With<UiPlacementSession>>`.
#[derive(Component, Default)]
#[require(PlacementSession)]
pub struct UiPlacementSession;

/// The object selected for placement.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
#[require(PlacementModes)]
pub struct PlacementObject(pub MapObject);

/// Enables replacement of existing objects of the same kind during placement.
#[derive(Component, Clone, Copy, Debug, Default)]
pub struct ReplaceExisting;

/// Visual validation state for [`UiPlacementSession`].
///
/// Domains update this immutable component with `insert_if_neq` so the placer redraws the overlay
/// on `On<Insert<PlacementFeedback>>`. Request handlers re-validate live state rather than reading
/// stored feedback.
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

/// Per-frame systems that update [`PlacementFeedback`] on [`UiPlacementSession`].
#[derive(SystemSet, Clone, Copy, Eq, PartialEq, Hash, Debug)]
pub struct PlacementFeedbackSystems;

/// Requests placement of the session's [`PlacementObject`] at its target [`GridArea`].
///
/// Domain observers match [`PlacementObject`], read the session's area and settings, validate
/// live world state, and report a [`PlaceReport`].
#[derive(EntityEvent, Clone, Copy, Debug)]
pub struct PlaceRequest {
    #[event_target]
    pub placement_session: Entity,
    pub response: ResponseRequest,
}
impl PlaceRequest {
    pub fn new(placement_session: Entity) -> Self {
        Self { placement_session, response: ResponseRequest::not_needed() }
    }

    pub fn with_response(mut self, response: ResponseRequest) -> Self {
        self.response = response;
        self
    }

    /// Reports `outcome` to [`Self::response`], mapping `Ok(())` to [`PlaceResult::Placed`].
    pub fn report(&self, commands: &mut Commands, outcome: Result<(), PlaceResult>) {
        let result = outcome.err().unwrap_or(PlaceResult::Placed);
        self.response.report(commands, |entity| PlaceReport { entity, result });
    }
}

/// Outcome reported for a [`PlaceRequest`].
#[derive(EntityEvent, Serialize)]
pub struct PlaceReport {
    #[serde(skip)]
    pub entity: Entity,
    pub result: PlaceResult,
}

#[derive(Serialize)]
pub enum PlaceResult {
    Placed,
    /// The target area fails the object's placement rules.
    Invalid,
    /// Available stock does not cover the object's cost.
    CannotAfford,
    /// The session is missing required components (such as [`GridArea`] or domain settings).
    InvalidSession,
}

/// Requests removal of content matching the session's [`PlacementObject`] at its target [`GridArea`].
///
/// Domain observers match [`PlacementObject`] and report a [`RemoveReport`]. Objects without a
/// removal observer ignore this event.
#[derive(EntityEvent, Clone, Copy, Debug)]
pub struct RemoveRequest {
    #[event_target]
    pub placement_session: Entity,
    pub response: ResponseRequest,
}
impl RemoveRequest {
    pub fn new(placement_session: Entity) -> Self {
        Self { placement_session, response: ResponseRequest::not_needed() }
    }

    pub fn with_response(mut self, response: ResponseRequest) -> Self {
        self.response = response;
        self
    }

    /// Reports `outcome` to [`Self::response`], mapping `Ok(())` to [`RemoveResult::Removed`].
    pub fn report(&self, commands: &mut Commands, outcome: Result<(), RemoveResult>) {
        let result = outcome.err().unwrap_or(RemoveResult::Removed);
        self.response.report(commands, |entity| RemoveReport { entity, result });
    }
}

/// Outcome reported for a [`RemoveRequest`].
#[derive(EntityEvent, Serialize)]
pub struct RemoveReport {
    #[serde(skip)]
    pub entity: Entity,
    pub result: RemoveResult,
}

#[derive(Serialize)]
pub enum RemoveResult {
    Removed,
    /// The target area contains no removable content of this kind.
    NothingToRemove,
    /// The session is missing required components (such as [`GridArea`]).
    InvalidSession,
}

/// Initializes a [`PlacementSession`] that has [`PlacementObject`] set.
///
/// The handling domain observer inserts [`GridArea`] at `origin`, overrides [`PlacementModes`] if
/// needed, and inserts default domain settings. If the session has [`UiPlacementSession`], the
/// observer also spawns its [`PlacementControls`]. Flushes before the placer builds the ghost so
/// ghost and control observers can read the initialized session components.
#[derive(EntityEvent, Clone, Copy, Debug)]
pub struct BeginPlacing {
    #[event_target]
    pub placement_session: Entity,
    pub origin: GridPosition,
}
impl BeginPlacing {
    pub fn new(placement_session: Entity, origin: GridPosition) -> Self {
        Self { placement_session, origin }
    }
}

/// Ends placement on a [`PlacementSession`].
///
/// Fired when exiting placement, before switching [`UiPlacementSession`] to a new object, or when
/// finishing a standalone session. [`UiPlacementSession`] is stripped back to its base markers and
/// its [`PlacementControls`] are despawned; standalone sessions are despawned.
#[derive(EntityEvent, Clone, Copy, Debug)]
pub struct StopPlacing {
    #[event_target]
    pub placement_session: Entity,
}
impl StopPlacing {
    pub fn new(placement_session: Entity) -> Self {
        Self { placement_session }
    }
}

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

/// Mouse input trigger modes for placing and removing the session's object.
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

/// Triggered when [`UiPlacementSession`] changes its object or [`GridArea`].
#[derive(Event, Clone, Copy, Debug)]
pub struct GridPlacerChanged;

/// Marker for domain UI controls spawned during placement; despawned on [`StopPlacing`].
#[derive(Component, Clone, Copy, Debug, Default)]
pub struct PlacementControls;

/// Requests a rebuild of the placement ghost after [`UiPlacementSession`] changes its object,
/// imprint, or visual settings.
#[derive(Event, Clone, Copy, Debug)]
pub struct GridPlacerGhostRebuildRequest;
