use std::marker::PhantomData;

use bevy::{ecs::system::SystemParam, prelude::*};

use game_core::prelude::{GridCoords, GridImprint, MapObject, ZDepth};

use crate::{energy_supply::EnergySupplyGrid, obstacles::{ObstacleGrid, ReservedCoords}, wisps::WispsGrid};

/// Event emitted when the grid placer's state changes (coords, imprint, etc).
/// Other systems can observe this to react to placer updates.
#[derive(Event, Clone, Copy, Debug)]
pub struct GridPlacerChanged;

/// Non-generic event emitted when the placer deactivates or switches to a different object type.
/// Domain UIs (e.g., QuantumField size selector) observe this to hide/cleanup.
#[derive(Event, Clone, Copy, Debug)]
pub struct StopPlacing;

/// Event to request modification of the grid placer's state.
/// Placer observes this and handles changes internally.
#[derive(Event, Clone, Copy, Debug)]
pub enum GridPlacerOverridePropertyRequest {
    OverrideImprint(GridImprint),
}

/// Generic placement request event. Domain observers listen for their specific T.
#[derive(Event)]
pub struct PlaceRequest<T>(PhantomData<T>);
impl<T> Default for PlaceRequest<T> {fn default() -> Self { Self(PhantomData) } }
impl<T> Clone for PlaceRequest<T> {fn clone(&self) -> Self { *self } }
impl<T> Copy for PlaceRequest<T> {}

/// Generic removal request event. Domain observers listen for their specific T.
#[derive(Event)]
pub struct RemoveRequest<T>(PhantomData<T>);
impl<T> Default for RemoveRequest<T> {fn default() -> Self { Self(PhantomData) } }
impl<T> Clone for RemoveRequest<T> {fn clone(&self) -> Self { *self } }
impl<T> Copy for RemoveRequest<T> {}

/// Generic event emitted when placement mode begins for a type.
/// Used by domains that need setup UI (e.g., QuantumField size selector).
#[derive(Event)]
pub struct BeginPlacing<T>(PhantomData<T>);
impl<T> Default for BeginPlacing<T> {fn default() -> Self { Self(PhantomData) } }
impl<T> Clone for BeginPlacing<T> {fn clone(&self) -> Self { *self } }
impl<T> Copy for BeginPlacing<T> {}

/// Trait for dynamic event dispatch. Stored as `Box<dyn PlacementEmitter>` in ObjectPlacementInfo.
pub trait PlacementEmitter: Send + Sync {
    fn emit(&self, commands: &mut Commands);
}
impl<T: Send + Sync + 'static> PlacementEmitter for PlaceRequest<T> {
    fn emit(&self, commands: &mut Commands) {
        commands.trigger(*self);
    }
}
impl<T: Send + Sync + 'static> PlacementEmitter for RemoveRequest<T> {
    fn emit(&self, commands: &mut Commands) {
        commands.trigger(*self);
    }
}
impl<T: Send + Sync + 'static> PlacementEmitter for BeginPlacing<T> {
    fn emit(&self, commands: &mut Commands) {
        commands.trigger(*self);
    }
}

/// Placement validity state returned by validators.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlacementValidity {
    Valid,
    ValidUnpowered,
    Invalid,
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
    pub place_emitter: Box<dyn PlacementEmitter>,
    pub remove_emitter: Option<Box<dyn PlacementEmitter>>,
    pub begin_placing_emitter: Option<Box<dyn PlacementEmitter>>,
    pub place_mode: PlacementMode,
    pub remove_mode: PlacementMode,
    /// Handle for the ghost preview image shown while placing. `None` falls back to a solid tinted shape.
    pub preview_image: Option<Handle<Image>>,
}

/// Active placement session data.
pub struct ActivePlacement {
    pub map_object: MapObject,
    pub placement_info: ObjectPlacementInfo,
}

#[derive(Component, Default)]
#[require(GridImprint, GridCoords, ZDepth = ZDepth(10.), crate::AutoGridTransformSync)]
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
    coords: GridCoords,
    imprint: GridImprint,
    grids: &GridsCollectionParam,
) -> PlacementValidity {
    if !coords.is_imprint_in_bounds(&imprint, grids.obstacle_grid.bounds()) {
        return PlacementValidity::Invalid;
    }
    if grids.reserved_coords.any_reserved(coords, imprint) {
        return PlacementValidity::Invalid;
    }
    if !grids.obstacle_grid.query_imprint_all(coords, imprint, |f| f.is_empty()) {
        return PlacementValidity::Invalid;
    }
    PlacementValidity::Valid
}

/// Generic annotator: when invalid, highlights cells that are out-of-bounds or non-empty as Negative.
/// Suitable for any object whose only placement constraint is that all cells must be empty.
pub fn annotate_non_empty(
    _: MapObject,
    coords: GridCoords,
    imprint: GridImprint,
    validity: PlacementValidity,
    map_data: &GridsCollectionParam,
) -> Vec<(GridCoords, CellHighlight)> {
    if validity != PlacementValidity::Invalid {
        return vec![];
    }
    imprint.iter(coords)
        .filter(|c| {
            !c.is_in_bounds(map_data.obstacle_grid.bounds())
                || !map_data.obstacle_grid[*c].is_empty()
        })
        .map(|c| (c, CellHighlight::Negative))
        .collect()
}
