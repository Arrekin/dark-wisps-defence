//! Shared render-graph wiring for the screen-space post-process passes.
//!
//! The post-process effects (ripple displacement, quantum field anomaly, force-field dome)
//! each register their own `ViewNode` in the `Core2d` graph under the label defined here.
//! Keeping the labels in lib-core lets every effect plugin reference them without reaching into
//! another feature module, and lets the *ordering* between the passes live in exactly one
//! place: [`PostProcessOrderingPlugin`].

use bevy::app::{App, Plugin};
use bevy::core_pipeline::core_2d::graph::{Core2d, Node2d};
use bevy::render::{
    render_graph::{RenderGraphExt, RenderLabel},
    RenderApp,
};

/// Ripple displacement pass — see `weaponry/ripple_post_process.rs`.
#[derive(Debug, Hash, PartialEq, Eq, Clone, RenderLabel)]
pub struct RipplePostProcessLabel;

/// Quantum field anomaly pass — see `map_objects/quantum_field_post_process.rs`.
#[derive(Debug, Hash, PartialEq, Eq, Clone, RenderLabel)]
pub struct QuantumFieldPostProcessLabel;

/// Force-field dome pass — see `weaponry/force_field_post_process.rs`.
#[derive(Debug, Hash, PartialEq, Eq, Clone, RenderLabel)]
pub struct ForceFieldPostProcessLabel;

/// Pins the screen-space post-process passes into one intentional order.
///
/// The passes form a ping-pong chain (each samples the previous one's output), so their order
/// decides layering wherever effects overlap on screen. The chosen order is:
///
/// `Tonemapping → Ripple → QuantumField → ForceField → EndMainPassPostProcessing`
///
/// i.e. ground ripple at the bottom, the quantum anomaly above it, and a force-field dome
/// composited on top — matching the physical reading of a floor anomaly under an aerial dome.
///
/// **Must be added after** the three effect plugins: render-graph edge creation panics on a
/// missing node, so every node has to be registered first. It expects all three passes present.
pub struct PostProcessOrderingPlugin;
impl Plugin for PostProcessOrderingPlugin {
    fn build(&self, app: &mut App) {
        let Some(render_app) = app.get_sub_app_mut(RenderApp) else {
            return;
        };
        render_app.add_render_graph_edges(
            Core2d,
            (
                Node2d::Tonemapping,
                RipplePostProcessLabel,
                ForceFieldPostProcessLabel,
                QuantumFieldPostProcessLabel,
                Node2d::EndMainPassPostProcessing,
            ),
        );
    }
}
