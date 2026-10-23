use std::marker::PhantomData;

use bevy::prelude::*;

/// Coalesces requests for a piece of work into one run of the system gated by
/// [`RequestFlag::is_requested`]. The marker type `M` names the work.
#[derive(Resource)]
pub struct RequestFlag<M: Send + Sync + 'static> {
    requested: bool,
    marker: PhantomData<M>,
}
impl<M: Send + Sync + 'static> Default for RequestFlag<M> {
    fn default() -> Self {
        Self { requested: false, marker: PhantomData }
    }
}
impl<M: Send + Sync + 'static> RequestFlag<M> {
    pub fn request(&mut self) {
        self.requested = true;
    }

    pub fn clear(&mut self) {
        self.requested = false;
    }

    /// Run condition.
    pub fn is_requested(flag: Res<Self>) -> bool {
        flag.requested
    }
}
