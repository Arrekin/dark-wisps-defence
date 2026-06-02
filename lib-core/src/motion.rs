//! # Motion metadata
//!
//! [`Locomotion`] records how an entity is *actually* moving, derived from its
//! `GlobalTransform` over time. A single generic system keeps it current, so any
//! entity — wisp or otherwise — can react to its real motion just by adding the
//! component.

use crate::lib_prelude::*;
use bevy::transform::TransformSystems;

pub mod motion_prelude {
    pub use super::{Locomotion, MotionSystems};
}

/// Scheduling handle for motion tracking, so dependents can order against it.
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub enum MotionSystems {
    /// [`Locomotion`] has been refreshed from this frame's transforms.
    Track,
}

pub struct MotionPlugin;
impl Plugin for MotionPlugin {
    fn build(&self, app: &mut App) {
        app
            .add_systems(
                PostUpdate,
                track_locomotion
                    .in_set(MotionSystems::Track)
                    .after(TransformSystems::Propagate),
            )
            ;
    }
}

/// How quickly measured velocity eases toward the latest sample (per second).
const VELOCITY_RESPONSE: f32 = 7.0;

/// Per-entity *measured* movement — the physical record of how an entity is
/// actually moving, derived from frame-to-frame `GlobalTransform` deltas and
/// lightly smoothed.
///
/// This is deliberately distinct from any intended-speed stat such as
/// `MovementSpeed`: it captures the motion that *actually happened*, including
/// things no stat reflects — a charge burst, being blocked against a wall, or the
/// diagonal of a grid step. Add it to any entity whose visuals or logic should
/// react to how it genuinely moves; [`track_locomotion`] keeps it up to date.
///
/// `last_position` is internal bookkeeping for the per-frame delta; read the
/// motion through [`velocity`](Self::velocity) / [`heading`](Self::heading) /
/// [`speed`](Self::speed).
#[derive(Component, Default)]
pub struct Locomotion {
    last_position: Option<Vec2>,
    velocity: Vec2,
}
impl Locomotion {
    /// Smoothed measured velocity, in world units per second.
    pub fn velocity(&self) -> Vec2 { self.velocity }
    /// Unit direction of travel, or zero while effectively still.
    pub fn heading(&self) -> Vec2 { self.velocity.normalize_or_zero() }
    /// Smoothed measured speed, in world units per second.
    pub fn speed(&self) -> f32 { self.velocity.length() }

    fn advance(&mut self, position: Vec2, dt: f32) {
        let Some(last) = self.last_position else {
            self.last_position = Some(position);
            return;
        };
        self.last_position = Some(position);
        if dt <= 0.0 { return; }
        let measured = (position - last) / dt;
        self.velocity = self.velocity.lerp(measured, (dt * VELOCITY_RESPONSE).min(1.0));
    }
}

/// Refreshes [`Locomotion`] for every entity that has it, from this frame's world
/// position. Runs after transform propagation so it reads the final position.
fn track_locomotion(
    time: Res<Time>,
    mut movers: Query<(&GlobalTransform, &mut Locomotion)>,
) {
    let dt = time.delta_secs();
    for (global_transform, mut locomotion) in movers.iter_mut() {
        locomotion.advance(global_transform.translation().truncate(), dt);
    }
}
