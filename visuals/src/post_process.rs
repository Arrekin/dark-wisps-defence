//! Shared `Core2d`-schedule wiring for the screen-space post-process passes.
//!
//! The post-process effects (ripple displacement, Inferno heat haze, Inferno dome distortion,
//! quantum field anomaly, force-field dome) each register their own system in the `Core2d`
//! schedule under the system set defined here.
//! Keeping the sets in one place lets every effect plugin reference them without reaching into
//! another feature module, and lets the *ordering* between the passes live in exactly one
//! place: `PostProcessOrderingPlugin` (in `visuals_internal`).

use bevy::ecs::schedule::SystemSet;

/// System set for the ripple displacement pass — see `weaponry_internal/src/ripple_post_process.rs`.
#[derive(SystemSet, Debug, Hash, PartialEq, Eq, Clone)]
pub struct RipplePostProcessSet;

/// System set for the Inferno's heat haze pass — see `buildings_internal/src/towers/emitter/tower_inferno_haze.rs`.
#[derive(SystemSet, Debug, Hash, PartialEq, Eq, Clone)]
pub struct InfernoHazePostProcessSet;

/// System set for the Inferno dome's distortion pass — see `weaponry_internal/src/inferno_dome_post_process.rs`.
#[derive(SystemSet, Debug, Hash, PartialEq, Eq, Clone)]
pub struct InfernoDomePostProcessSet;

/// System set for the quantum field anomaly pass — see `map_objects_internal/src/quantum_field_post_process.rs`.
#[derive(SystemSet, Debug, Hash, PartialEq, Eq, Clone)]
pub struct QuantumFieldPostProcessSet;

/// System set for the force-field dome pass — see `weaponry_internal/src/force_field_post_process.rs`.
#[derive(SystemSet, Debug, Hash, PartialEq, Eq, Clone)]
pub struct ForceFieldPostProcessSet;
