use std::marker::PhantomData;

use crate::lib_prelude::*;

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

impl<T> Default for PlaceRequest<T> {
    fn default() -> Self {
        Self(PhantomData)
    }
}

impl<T> Clone for PlaceRequest<T> {
    fn clone(&self) -> Self {
        Self(PhantomData)
    }
}

impl<T> Copy for PlaceRequest<T> {}

/// Generic removal request event. Domain observers listen for their specific T.
#[derive(Event)]
pub struct RemoveRequest<T>(PhantomData<T>);

impl<T> Default for RemoveRequest<T> {
    fn default() -> Self {
        Self(PhantomData)
    }
}

impl<T> Clone for RemoveRequest<T> {
    fn clone(&self) -> Self {
        Self(PhantomData)
    }
}

impl<T> Copy for RemoveRequest<T> {}

/// Generic event emitted when placement mode begins for a type.
/// Used by domains that need setup UI (e.g., QuantumField size selector).
#[derive(Event)]
pub struct BeginPlacing<T>(PhantomData<T>);

impl<T> Default for BeginPlacing<T> {
    fn default() -> Self {
        Self(PhantomData)
    }
}

impl<T> Clone for BeginPlacing<T> {
    fn clone(&self) -> Self {
        Self(PhantomData)
    }
}

impl<T> Copy for BeginPlacing<T> {}

/// Trait for dynamic event dispatch. Stored as `Box<dyn PlacementEmitter>` in ObjectPlacementInfo.
pub trait PlacementEmitter: Send + Sync {
    fn emit(self: Box<Self>, commands: &mut Commands);
    fn clone_box(&self) -> Box<dyn PlacementEmitter>;
}

impl<T: Send + Sync + 'static> PlacementEmitter for PlaceRequest<T> {
    fn emit(self: Box<Self>, commands: &mut Commands) {
        commands.trigger(*self);
    }

    fn clone_box(&self) -> Box<dyn PlacementEmitter> {
        Box::new(*self)
    }
}

impl<T: Send + Sync + 'static> PlacementEmitter for RemoveRequest<T> {
    fn emit(self: Box<Self>, commands: &mut Commands) {
        commands.trigger(*self);
    }

    fn clone_box(&self) -> Box<dyn PlacementEmitter> {
        Box::new(*self)
    }
}

impl<T: Send + Sync + 'static> PlacementEmitter for BeginPlacing<T> {
    fn emit(self: Box<Self>, commands: &mut Commands) {
        commands.trigger(*self);
    }

    fn clone_box(&self) -> Box<dyn PlacementEmitter> {
        Box::new(*self)
    }
}
