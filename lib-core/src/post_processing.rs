//! Shared `Core2d`-schedule wiring for the screen-space post-process passes.
//!
//! The post-process effects (ripple displacement, quantum field anomaly, force-field dome)
//! each register their own system in the `Core2d` schedule under the system set defined here.
//! Keeping the sets in lib-core lets every effect plugin reference them without reaching into
//! another feature module, and lets the *ordering* between the passes live in exactly one
//! place: [`PostProcessOrderingPlugin`].

use bevy::app::{App, Plugin};
use bevy::core_pipeline::{Core2dSystems, schedule::Core2d, tonemapping::tonemapping};
use bevy::ecs::schedule::{IntoScheduleConfigs, SystemSet};
use bevy::render::RenderApp;

/// System set for the ripple displacement pass — see `weaponry/ripple_post_process.rs`.
#[derive(SystemSet, Debug, Hash, PartialEq, Eq, Clone)]
pub struct RipplePostProcessSet;

/// System set for the quantum field anomaly pass — see `map_objects/quantum_field_post_process.rs`.
#[derive(SystemSet, Debug, Hash, PartialEq, Eq, Clone)]
pub struct QuantumFieldPostProcessSet;

/// System set for the force-field dome pass — see `weaponry/force_field_post_process.rs`.
#[derive(SystemSet, Debug, Hash, PartialEq, Eq, Clone)]
pub struct ForceFieldPostProcessSet;

/// Pins the screen-space post-process passes into one intentional order.
///
/// The passes form a ping-pong chain (each samples the previous one's output), so their order
/// decides layering wherever effects overlap on screen. The chosen order is:
///
/// `Tonemapping → Ripple → ForceField → QuantumField → Upscaling`
///
/// i.e. ground ripple at the bottom, the force-field dome above it, and the quantum anomaly
/// composited on top, so the anomaly's reality-warp distorts everything beneath it.
/// Upscaling runs after all post-processing via Bevy's built-in `Core2d` schedule.
///
/// **Must be added after** the three effect plugins: the system sets must already be
/// configured by each effect plugin before the ordering constraints are applied here.
pub struct PostProcessOrderingPlugin;
impl Plugin for PostProcessOrderingPlugin {
    fn build(&self, app: &mut App) {
        let Some(render_app) = app.get_sub_app_mut(RenderApp) else {
            return;
        };
        render_app.configure_sets(
            Core2d,
            (
                RipplePostProcessSet
                    .in_set(Core2dSystems::PostProcess)
                    .after(tonemapping),
                ForceFieldPostProcessSet
                    .in_set(Core2dSystems::PostProcess)
                    .after(RipplePostProcessSet),
                QuantumFieldPostProcessSet
                    .in_set(Core2dSystems::PostProcess)
                    .after(ForceFieldPostProcessSet),
            ),
        );
    }
}
