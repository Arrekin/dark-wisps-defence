use bevy::{ecs::system::SystemParam, prelude::*};

use game_core::prelude::{GridCoords, GridImprint, MapObject, ZDepth};

use crate::{energy_supply::EnergySupplyGrid, obstacles::{ObstacleGrid, ReservedCoords}, wisps::WispsGrid};

/// Event emitted when the grid placer's state changes (coords, imprint, etc).
/// Other systems can observe this to react to placer updates.
#[derive(Event, Clone, Copy, Debug)]
pub struct GridPlacerChanged;

/// Emitted when placement begins for an object.
///
/// Domain UI observes this event to create controls such as size or style selectors. Imprint and
/// style changes within the same session do not emit another event.
#[derive(Event, Clone, Copy, Debug)]
pub struct BeginPlacing(pub MapObject);

/// Non-generic event emitted when the placer deactivates or switches to a different object type.
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
/// in `assets/shaders/grid_placer.wgsl`; the two must stay in step.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u32)]
pub enum PlacementValidity {
    Valid = 0,
    ValidUnpowered = 1,
    Invalid = 2,
}
impl PlacementValidity {
    pub fn shader_index(self) -> u32 {
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

/// Bevy SystemParam that bundles all grid resources needed for placement validation.
/// `reserved_coords` is `ResMut` so place request handlers can also call `.reserve()` on it.
#[derive(SystemParam)]
pub struct GridsCollectionParam<'w> {
    pub obstacle_grid: Res<'w, ObstacleGrid>,
    pub energy_supply_grid: Res<'w, EnergySupplyGrid>,
    pub reserved_coords: ResMut<'w, ReservedCoords>,
    pub wisps_grid: Res<'w, WispsGrid>,
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

/// Determines whether the current position is a valid placement site.
/// Returns only validity — cell highlights are the annotator's responsibility.
pub type PlacementValidatorFn = fn(MapObject, GridCoords, GridImprint, &GridsCollectionParam) -> PlacementValidity;

/// Produces per-cell highlights for the placement ghost.
/// Receives the validity result so it can decide which highlights are appropriate
/// (e.g. negative cells only when invalid, positive cells only when valid).
pub type PlacementAnnotatorFn = fn(MapObject, GridCoords, GridImprint, PlacementValidity, &GridsCollectionParam) -> Vec<(GridCoords, CellHighlight)>;

/// Generic placement info extracted from Almanach. The placer stores this without needing to
/// know about specific structure types.
pub struct ObjectPlacementInfo {
    pub imprint: GridImprint,
    pub validate: PlacementValidatorFn,
    pub annotate: PlacementAnnotatorFn,
    pub placement: PlacementModes,
}

/// Active placement session data.
pub struct ActivePlacement {
    pub map_object: MapObject,
    pub placement_info: ObjectPlacementInfo,
}

/// Which variant of the object is being placed. The placer carries the number and never
/// interprets it; the domain that owns the object decides what it selects. Reset when a
/// placement session begins, so a choice made for one object does not carry to the next.
#[derive(Component, Clone, Copy, Default)]
pub struct PlacementStyle(pub u32);

#[derive(Component, Default)]
#[require(GridImprint, GridCoords, PlacementStyle, ZDepth::GRID_PLACER, crate::AutoGridTransformSync)]
pub struct GridObjectPlacer {
    pub active_placement: Option<ActivePlacement>,
}

impl GridObjectPlacer {
    pub fn map_object(&self) -> Option<MapObject> {
        self.active_placement.as_ref().map(|a| a.map_object)
    }
}

#[derive(Resource, Default)]
pub struct GridObjectPlacerRequest(Option<MapObject>);
impl GridObjectPlacerRequest {
    pub fn is_set(&self) -> bool { self.0.is_some() }
    pub fn set(&mut self, request: MapObject) { self.0 = Some(request); }
    pub fn take(&mut self) -> Option<MapObject> { self.0.take() }

    pub fn there_is_request() -> fn(Res<GridObjectPlacerRequest>) -> bool {
        |placer_request: Res<GridObjectPlacerRequest>| placer_request.is_set()
    }
}

/// Generic validator: checks bounds, reserved coords, and that all cells are empty.
pub fn validator_all_empty(
    _: MapObject,
    origin: GridCoords,
    imprint: GridImprint,
    grids: &GridsCollectionParam,
) -> PlacementValidity {
    if !imprint.is_in_bounds(origin, grids.obstacle_grid.bounds) {
        return PlacementValidity::Invalid;
    }
    if grids.reserved_coords.any_reserved(origin, imprint) {
        return PlacementValidity::Invalid;
    }
    if !grids.obstacle_grid.query_imprint_all(origin, imprint, |f| f.is_empty()) {
        return PlacementValidity::Invalid;
    }
    PlacementValidity::Valid
}

/// Generic annotator: when invalid, highlights cells that are out-of-bounds or non-empty as Negative.
/// Suitable for any object whose only placement constraint is that all cells must be empty.
pub fn annotate_non_empty(
    _: MapObject,
    origin: GridCoords,
    imprint: GridImprint,
    validity: PlacementValidity,
    map_data: &GridsCollectionParam,
) -> Vec<(GridCoords, CellHighlight)> {
    if validity != PlacementValidity::Invalid {
        return vec![];
    }
    imprint.iter(origin)
        .filter(|coords| {
            !coords.are_in_bounds(map_data.obstacle_grid.bounds)
                || !map_data.obstacle_grid[*coords].is_empty()
        })
        .map(|coords| (coords, CellHighlight::Negative))
        .collect()
}
